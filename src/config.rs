use std::fs;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
}

pub fn load_config() -> Config {
    let config_str =
        fs::read_to_string("config.toml").expect("Failed to open config file at config.toml.");
    toml::from_str(&config_str)
        .expect("Failed to open config.toml, are you sure it is correct toml?")
}
