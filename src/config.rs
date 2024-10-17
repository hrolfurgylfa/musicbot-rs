use core::panic;

use serde::Deserialize;

fn default_max_previously_played() -> usize {
    5
}

fn default_prefix() -> String {
    "=".to_owned()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_prefix")]
    pub prefix: String,
    pub token: String,
    pub error_webhook: Option<String>,
    #[serde(default = "default_max_previously_played")]
    pub max_previously_played: usize,
}

pub fn load_config() -> Config {
    let config_str =
        fs::read_to_string("config.toml").expect("Failed to open config file at config.toml.");

    match toml::from_str(&config_str) {
        Ok(config) => config,
        Err(err) => panic!("Failed to parse config: {}", err),
    }
}
