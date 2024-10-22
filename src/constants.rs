use alloy::primitives::{address, Address};

// 1f192c261D463cD1c4E0B2F5696452448DC47506 - ONE MORE POSSIBLE CLAIMER ADDRESS | HOLDS 1000000000 $SCR
// E8bE8eB940c0ca3BD19D911CD3bEBc97Bea0ED62 - HOLDS 97 $SCR
pub const CLAIMER_CONTRACT_ADDRESS: Address = address!("E8bE8eB940c0ca3BD19D911CD3bEBc97Bea0ED62");
pub const REQUEST_PROOF_URL: &str = "https://claim.scroll.io/";
pub const TOKEN_CONTRACT_ADDRESS: Address = address!("d29687c813D741E2F938F4aC377128810E217b1b");

// FILES
pub const PRIVATE_KEYS_FILE_PATH: &str = "data/private_keys.txt";
pub const RECIPIENTS_FILE_PATH: &str = "data/recipients.txt";

pub const SCROLL_CHAIN_ID: u64 = 534352;
