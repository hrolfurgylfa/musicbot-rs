use std::sync::Arc;

use serenity::all::{Cache, CacheHttp, GuildId, Http};
use tracing::error;

#[derive(Debug, Clone)]
pub struct SerenityQuery {
    http: Arc<Http>,
    cache: Arc<Cache>,
}

impl SerenityQuery {
    pub fn new(http: Arc<Http>, cache: Arc<Cache>) -> Self {
        SerenityQuery { http, cache }
    }

    pub async fn get_guild_name(&self, id: GuildId) -> Option<String> {
        let maybe_guild = self.cache.guild(id);
        if let Some(guild) = maybe_guild {
            Some(guild.name.clone())
        } else {
            drop(maybe_guild);
            match self.http.get_guild(id).await {
                Ok(guild) => Some(guild.name),
                Err(err) => {
                    error!(?self.http, err = %err, "Failed to fetch guild with Http.");
                    None
                }
            }
        }
    }
}

impl CacheHttp for SerenityQuery {
    fn http(&self) -> &Http {
        &self.http
    }
    fn cache(&self) -> Option<&Arc<Cache>> {
        Some(&self.cache)
    }
}

impl<A, B> From<&poise::Context<'_, A, B>> for SerenityQuery {
    fn from(value: &poise::Context<'_, A, B>) -> Self {
        let http = value.serenity_context().http.clone();
        let cache = value.serenity_context().cache.clone();
        SerenityQuery::new(http, cache)
    }
}

impl From<&serenity::all::Context> for SerenityQuery {
    fn from(value: &serenity::all::Context) -> Self {
        let http = value.http.clone();
        let cache = value.cache.clone();
        SerenityQuery::new(http, cache)
    }
}

impl From<&serenity::client::Client> for SerenityQuery {
    fn from(value: &serenity::client::Client) -> Self {
        let http = value.http.clone();
        let cache = value.cache.clone();
        SerenityQuery::new(http, cache)
    }
}
