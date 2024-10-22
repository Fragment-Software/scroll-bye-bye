use std::{sync::Arc, time::Duration};

use alloy::{
    network::{Ethereum, EthereumWallet, NetworkWallet, TransactionBuilder},
    primitives::{Address, Bytes, FixedBytes, U256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::{client::ClientBuilder, types::TransactionRequest},
    sol,
    sol_types::SolCall,
    transports::{http::Http, layers::RetryBackoffLayer, Transport},
};
use alloy_chains::NamedChain;
use rand::{seq::SliceRandom, thread_rng};
use tokio::task::JoinSet;

use TokenDistributor::claimCall;
use IERC20::transferCall;

use crate::{
    config::Config,
    constants::{CLAIMER_CONTRACT_ADDRESS, SCROLL_CHAIN_ID, TOKEN_CONTRACT_ADDRESS},
    proof::{extract_proof_and_amount, get_proof},
    utils::{read_private_keys, read_recipients},
};

sol! {
    #[sol(rpc)]
    contract TokenDistributor {
        function claim(address _account, uint256 _amount, bytes32[] calldata _merkleProof) external;
        mapping(address user => bool claimed) public hasClaimed;
    }

    #[sol(rpc)]
    #[derive(Debug, PartialEq, Eq)]
    contract IERC20 {
        struct PartialDelegation {
          address _delegatee;
          uint96 _numerator;
        }

        function delegate(PartialDelegation[] calldata _partialDelegations) public virtual;

        mapping(address account => uint256) public balanceOf;

        function transfer(address to, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
}

const SCROLL_EXPLORER_URL: &str = "https://scrollscan.com";

pub async fn send_transaction<P, T, W>(
    provider: P,
    wallet: Arc<W>,
    to: Address,
    input: Option<Bytes>,
    value: U256,
) -> eyre::Result<bool>
where
    P: Provider<T, Ethereum>,
    T: Transport + Clone,
    W: NetworkWallet<Ethereum>,
{
    let eip1559_fees = provider.estimate_eip1559_fees(None).await?;
    let from = wallet.default_signer_address();

    let nonce = provider.get_transaction_count(from).await?;

    let mut tx_request = TransactionRequest::default()
        .with_max_fee_per_gas(eip1559_fees.max_fee_per_gas)
        .with_max_priority_fee_per_gas(eip1559_fees.max_priority_fee_per_gas)
        .with_to(to)
        .with_value(value)
        .with_nonce(nonce)
        .with_chain_id(SCROLL_CHAIN_ID)
        .with_from(from);

    if let Some(data) = input {
        tx_request.set_input(data);
    }

    let gas_limit = provider.estimate_gas(&tx_request).await?;
    tx_request.set_gas_limit(gas_limit);

    let signed_transaction = tx_request.build(&wallet).await?;
    let pending_tx = provider.send_tx_envelope(signed_transaction).await?;
    let receipt = pending_tx.get_receipt().await?;

    let url = format!("{SCROLL_EXPLORER_URL}/tx/{}", receipt.transaction_hash);

    if receipt.status() {
        tracing::info!("Transaction successful: {}", url);
    } else {
        tracing::error!("Transaction failed: {}", url);
    }

    Ok(receipt.status())
}

pub async fn transfer<P, T, W>(
    provider: Arc<P>,
    wallet: Arc<W>,
    to: Address,
    value: U256,
) -> eyre::Result<bool>
where
    P: Provider<T, Ethereum>,
    T: Transport + Clone,
    W: NetworkWallet<Ethereum>,
{
    let address = wallet.default_signer_address();
    tracing::info!("Sending {value} $SCR from {address} to {to}",);
    let input = transferCall { to, amount: value }.abi_encode();

    send_transaction(
        provider,
        wallet,
        TOKEN_CONTRACT_ADDRESS,
        Some(input.into()),
        U256::from(0),
    )
    .await
}

pub async fn claim<P, T, W>(
    provider: Arc<P>,
    wallet: Arc<W>,
    amount: U256,
    proof: Vec<FixedBytes<32>>,
) -> eyre::Result<bool>
where
    P: Provider<T, Ethereum>,
    T: Transport + Clone,
    W: NetworkWallet<Ethereum>,
{
    let address = wallet.default_signer_address();
    tracing::info!("Claiming {amount} for {address}");

    let input = claimCall {
        _account: address,
        _amount: amount,
        _merkleProof: proof,
    }
    .abi_encode();

    send_transaction(
        provider,
        wallet,
        CLAIMER_CONTRACT_ADDRESS,
        Some(input.into()),
        U256::from(0),
    )
    .await
}

pub async fn get_token_balance<P, T>(
    provider: Arc<P>,
    address: Address,
    token_contract_address: Address,
) -> eyre::Result<U256>
where
    P: Provider<T, Ethereum>,
    T: Transport + Clone,
{
    let contract_instance = IERC20::new(token_contract_address, provider.clone());
    let balance = contract_instance.balanceOf(address).call().await?._0;

    Ok(balance)
}

pub async fn claim_and_transfer<P, T, W>(
    wallet: Arc<W>,
    provider: Arc<P>,
    recipient: Address,
) -> eyre::Result<()>
where
    P: Provider<T, Ethereum>,
    T: Transport + Clone,
    W: NetworkWallet<Ethereum>,
{
    let distributor_contract_instance =
        TokenDistributor::new(CLAIMER_CONTRACT_ADDRESS, provider.clone());

    let wallet_address = wallet.default_signer_address();
    let has_claimed = distributor_contract_instance
        .hasClaimed(wallet_address)
        .call()
        .await?
        .claimed;

    let allocation = match has_claimed {
        true => get_token_balance(provider.clone(), wallet_address, TOKEN_CONTRACT_ADDRESS).await?,
        false => {
            let response = get_proof(wallet_address).await?; // TODO: request proof and allocation from the API
            let (proof, allocation) = extract_proof_and_amount(&response)?;
            claim(provider.clone(), wallet.clone(), allocation, proof).await?;

            tokio::time::sleep(Duration::from_millis(500)).await;

            allocation
        }
    };

    if allocation != U256::ZERO {
        transfer(provider, wallet, recipient, allocation).await?;
    }

    Ok(())
}

pub async fn claim_for_all(config: Config) {
    let mut rng = thread_rng();

    let init_providers = |rpc_urls: Vec<String>| -> Vec<_> {
        let retry_layer = RetryBackoffLayer::new(10, 2, 500);

        rpc_urls
            .into_iter()
            .map(|rpc_url| {
                let client = ClientBuilder::default()
                    .layer(retry_layer.clone())
                    .transport(Http::new(rpc_url.parse().unwrap()), false);

                Arc::new(
                    ProviderBuilder::new()
                        .with_recommended_fillers()
                        .with_chain(NamedChain::Scroll)
                        .on_provider(RootProvider::new(client)),
                )
            })
            .collect()
    };

    let providers = init_providers(config.rpc_urls.clone());
    let wallets = read_private_keys().await;
    let recipients = read_recipients().await;

    let mut handles = JoinSet::new();

    for (wallet, recipient) in wallets.into_iter().zip(recipients.into_iter()) {
        tokio::time::sleep(Duration::from_millis(config.spawn_task_delay)).await;
        let provider = providers.choose(&mut rng).unwrap().clone();

        handles.spawn(async move {
            let task_result = claim_and_transfer(wallet.clone(), provider, recipient).await;
            (wallet, recipient, task_result)
        });
    }

    while let Some(res) = handles.join_next().await {
        let (wallet, recipient, task_result) = res.unwrap();
        let address =
            <Arc<EthereumWallet> as NetworkWallet<Ethereum>>::default_signer_address(&wallet);

        match task_result {
            Ok(_) => tracing::info!("Claimed and transferred: {address}",),
            Err(e) => {
                tracing::error!("Claim or transfer failed with error {e}. Address: {address}");
                let provider = providers.choose(&mut rng).unwrap().clone();

                handles.spawn(async move {
                    let task_result = claim_and_transfer(wallet.clone(), provider, recipient).await;
                    (wallet, recipient, task_result)
                });
            }
        }
    }
}
