use std::sync::Arc;

use serenity::{all::GuildId, async_trait};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, Songbird};

use crate::Data;

pub struct TrackErrorNotifier;

#[async_trait]
impl VoiceEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        println!("Act");
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                println!(
                    "Track {:?} encountered an error: {:?}",
                    handle.uuid(),
                    state.playing
                );
            }
        }

        None
    }
}

pub struct TrackStopHandler {
    songbird: Arc<Songbird>,
    data: Data,
    guild_id: GuildId,
}

impl TrackStopHandler {
    pub fn new(songbird: Arc<Songbird>, data: Data, guild_id: GuildId) -> TrackStopHandler {
        TrackStopHandler {
            songbird,
            data,
            guild_id,
        }
    }
}

#[async_trait]
impl VoiceEventHandler for TrackStopHandler {
    async fn act(&self, _: &EventContext<'_>) -> Option<Event> {
        let data = self.data.clone();
        let songbird = self.songbird.clone();
        let guild_id = self.guild_id;
        match data.next_song(songbird, guild_id, false).await {
            Ok(_) => (),
            Err(e) => println!("Error switching to next song: {}", e),
        };

        None
    }
}
