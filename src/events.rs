use std::sync::Arc;

use serenity::{all::GuildId, async_trait};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};

use crate::{playlist_info::get_server_info, Data, Song, TrackData};

pub struct TrackErrorNotifier;

#[async_trait]
impl VoiceEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                let data = handle.data::<TrackData>();
                let url = data.url.as_deref().unwrap_or("Unknown");
                tracing::error!(?handle, ?state, "Track \"{}\" encountered an error.", url);
            }
        }

        None
    }
}

pub struct TrackEndNotifier {
    data: Arc<Data>,
    guild_id: GuildId,
}

impl TrackEndNotifier {
    pub fn new(data: Arc<Data>, guild_id: GuildId) -> TrackEndNotifier {
        TrackEndNotifier { data, guild_id }
    }
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (_state, handle) in *track_list {
                let song = {
                    let data = handle.data::<TrackData>();
                    Song {
                        title: data.title.clone(),
                        url: data.url.clone(),
                    }
                };

                let server_info_lock = get_server_info(self.data.clone(), self.guild_id).await;
                let mut server_info = server_info_lock.lock();
                server_info.previous_songs.push_back(song);
                while server_info.previous_songs.len() > self.data.config.max_previously_played {
                    server_info.previous_songs.pop_front();
                }
            }
        }

        None
    }
}
