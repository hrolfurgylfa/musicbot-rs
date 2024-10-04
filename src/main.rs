use std::{collections::HashMap, sync::Arc};

use serenity::{async_trait, futures::lock::Mutex};
// This trait adds the `register_songbird` and `register_songbird_with` methods
// to the client builder below, making it easy to install this voice client.
// The voice client can be retrieved in any command using `songbird::get(ctx).await`.
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, SerenityInit, Songbird};

// YtDl requests need an HTTP client to operate -- we'll create and store our own.
use reqwest::Client as HttpClient;

use serenity::prelude::{GatewayIntents, TypeMapKey};

mod config;
use config::load_config;

mod commands;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Debug)]
struct Data {
    // queues: Mutex<HashMap<u32, Queue>>,
    songbird: Arc<Songbird>,
}

#[derive(Debug)]
struct Queue {
    songs: Vec<Song>,
}

#[derive(Debug)]
struct Song {
    name: String,
    url: String,
}

struct TrackErrorNotifier;

#[async_trait]
impl VoiceEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
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

struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

struct Handler;

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx, .. } => {
            println!("Error in command `{}`: {:?}", ctx.command().name, error);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                println!("Error while handling error: {}", e)
            }
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = load_config();

    let options = poise::FrameworkOptions {
        commands: vec![
            commands::help(),
            commands::join(),
            commands::leave(),
            commands::play(),
        ],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("=".to_owned()),
            edit_tracker: None,
            ..Default::default()
        },
        on_error: |error| Box::pin(on_error(error)),
        ..Default::default()
    };

    let songbird = songbird::Songbird::serenity();
    let songbird_clone = songbird.clone();
    let framework = poise::Framework::builder()
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                // songbird_clone.initialise_client_data(1, ready.user.id);
                Ok(Data {
                    // queues: Mutex::new(HashMap::new()),
                    songbird: songbird_clone,
                })
            })
        })
        .options(options)
        .build();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let client = serenity::client::Client::builder(&config.token, intents)
        .voice_manager_arc(songbird)
        .framework(framework)
        .type_map_insert::<HttpKey>(HttpClient::new())
        .await;

    client.unwrap().start().await.unwrap()
}
