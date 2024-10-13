use std::time::Duration;

use reqwest::Client as HttpClient;
use serenity::prelude::TypeMapKey;

pub struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

pub struct SongTitleKey;

impl TypeMapKey for SongTitleKey {
    type Value = String;
}

pub struct SongUrlKey;

impl TypeMapKey for SongUrlKey {
    type Value = String;
}

pub struct SongLengthKey;

impl TypeMapKey for SongLengthKey {
    type Value = Duration;
}
