use std::{path::Path, str::FromStr, sync::Arc};

use alloy::{network::EthereumWallet, primitives::Address, signers::local::PrivateKeySigner};

use tokio::io::AsyncBufReadExt;

use crate::constants::{PRIVATE_KEYS_FILE_PATH, RECIPIENTS_FILE_PATH};

pub async fn read_file_lines(path: impl AsRef<Path>) -> eyre::Result<Vec<String>> {
    let file = tokio::fs::read(path).await?;
    let mut lines = file.lines();

    let mut contents = vec![];
    while let Some(line) = lines.next_line().await? {
        contents.push(line);
    }

    Ok(contents)
}

pub async fn read_private_keys() -> Vec<Arc<EthereumWallet>> {
    read_file_lines(PRIVATE_KEYS_FILE_PATH)
        .await
        .expect("Private keys file to be present")
        .iter()
        .map(|pk| {
            let signer = PrivateKeySigner::from_str(pk).expect("Private key to be valid");
            Arc::new(EthereumWallet::new(signer))
        })
        .collect()
}

pub async fn read_recipients() -> Vec<Address> {
    read_file_lines(RECIPIENTS_FILE_PATH)
        .await
        .expect("Recipients file must be present")
        .iter()
        .map(|a| Address::from_str(a).expect("Recipinet address to be valid"))
        .collect()
}
