use std::{fs, sync::Arc};

use serde::Deserialize;

pub type Config = Arc<MainConfig>;

#[derive(Debug, Clone, Deserialize)]
pub struct MainConfig {
    pub token: String,
    pub error_webhook: Option<String>,
}

pub fn load_config() -> Config {
    let config_str =
        fs::read_to_string("config.toml").expect("Failed to open config file at config.toml.");
    Arc::new(
        toml::from_str(&config_str)
            .expect("Failed to open config.toml, are you sure it is correct toml?"),
    )
}
