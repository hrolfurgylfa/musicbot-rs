use serenity::async_trait;
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};

use crate::typekeys::SongUrlKey;

pub struct TrackErrorNotifier;

#[async_trait]
impl VoiceEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                let typemap = handle.typemap().read().await;
                let url = typemap
                    .get::<SongUrlKey>()
                    .map(|src| src.as_str())
                    .unwrap_or("Unknown");
                tracing::error!(?handle, ?state, "Track \"{}\" encountered an error.", url);
            }
        }

        None
    }
}
