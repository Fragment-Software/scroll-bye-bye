use claimer::claim_for_all;
use config::Config;

use logger::init_default_logger;

mod claimer;
mod config;
mod constants;
mod logger;
mod proof;
mod utils;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _guard = init_default_logger();

    let config = Config::read_default().await;

    claim_for_all(config).await;

    Ok(())
}
