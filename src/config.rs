use rand::{rngs::ThreadRng, seq::SliceRandom};
use reqwest::Proxy;
use serde::Deserialize;
use std::path::Path;

const CONFIG_FILE_PATH: &str = "data/config.toml";

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Config {
    pub rpc_urls: Vec<String>,
    pub spawn_task_delay: u64,
    pub proxies: Vec<String>,
}

impl Config {
    async fn read_from_file(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let cfg_str = tokio::fs::read_to_string(path).await?;
        Ok(toml::from_str(&cfg_str)?)
    }

    pub async fn read_default() -> Self {
        Self::read_from_file(CONFIG_FILE_PATH)
            .await
            .expect("Default config to be valid")
    }

    pub fn get_random_proxy(&self, rng: &mut ThreadRng) -> reqwest::Proxy {
        let proxy = self.proxies.choose(rng).unwrap().clone();

        Proxy::all(proxy).unwrap()
    }
}
