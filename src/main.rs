use std::{
    collections::{HashMap, VecDeque},
    fmt::Write,
    sync::Arc,
};

use parking_lot::{Mutex, RwLock};
use reqwest::Client as HttpClient;

use serenity::{
    all::{ChannelId, GuildId, Http, MessageId},
    prelude::GatewayIntents,
};
use songbird::SerenityInit;

use tracing_appender;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Layer, Registry};

mod config;
use config::{load_config, Config};

mod commands;

mod events;

mod trimmed_embed;

mod typekeys;
use typekeys::HttpKey;

mod tracing_webhook;

mod playlist_info;
use playlist_info::start_queue_message_update;

mod serenity_query;

mod deadlock_detection;
use deadlock_detection::start_deadlock_detection;

#[derive(Debug, Clone)]
struct Song {
    title: String,
    url: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct MsgLocation {
    channel_id: ChannelId,
    message_id: MessageId,
}

impl MsgLocation {
    pub fn new(channel_id: ChannelId, message_id: MessageId) -> Self {
        MsgLocation {
            channel_id,
            message_id,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ServerInfo {
    status_message: Option<MsgLocation>,
    previous_songs: VecDeque<Song>,
}

#[derive(Debug)]
struct Data {
    server_info: RwLock<HashMap<GuildId, Arc<Mutex<ServerInfo>>>>,
    commands_info: RwLock<String>,
    config: Config,
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Arc<Data>, Error>;

async fn get_songbird_manager(ctx: Context<'_>) -> Arc<songbird::Songbird> {
    songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone()
}

async fn on_error(error: poise::FrameworkError<'_, Arc<Data>, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx, .. } => {
            tracing::error!(err = %error, "Error in command \"{}\": {:?}", ctx.command().name, error);
            if let Err(e) = ctx.say("Error running command, please contact Hroi.").await {
                tracing::error!("Failed to warn user of crashed command: {}", e);
            }
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                tracing::error!("Error while handling error: \"{}\":", e);
            } else {
                tracing::error!("Unknown error in poise.");
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let config = load_config();
    let http = Http::new(&config.token);
    let data = Arc::new(Data {
        server_info: RwLock::new(HashMap::new()),
        commands_info: RwLock::new("".to_owned()),
        config: config.clone(),
    });
    let data_clone = data.clone();

    // Setup logging
    let file_appender = tracing_appender::rolling::daily("./logs", "music_bot.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = Registry::default()
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(file_writer)
                .with_filter(EnvFilter::new("debug,music_bot=trace")),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(std::io::stdout)
                .with_filter(EnvFilter::new("info,music_bot=debug")),
        )
        .with(console_subscriber::spawn())
        .with(tracing_webhook::Layer::build(config.error_webhook, http));
    tracing::subscriber::set_global_default(subscriber).expect("unable to set global subscriber");

    let options = poise::FrameworkOptions {
        commands: vec![
            commands::help(),
            commands::join(),
            commands::leave(),
            commands::play(),
            commands::queue(),
            commands::skip(),
        ],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some(config.prefix),
            edit_tracker: None,
            ..Default::default()
        },
        on_error: |error| Box::pin(on_error(error)),
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                // Update the commands list string
                let mut longest_commnd_len = 0;
                for cmd in &framework.options().commands {
                    if cmd.name.len() > longest_commnd_len {
                        longest_commnd_len = cmd.name.len();
                    }
                }
                let mut help_text = "".to_owned();
                for cmd in &framework.options().commands {
                    let description = cmd.description.as_deref().unwrap_or("");
                    let padding = " ".repeat(longest_commnd_len - cmd.name.len());
                    write!(help_text, "/{}{}  - {}\n", cmd.name, padding, description).unwrap();
                }
                {
                    let mut commands_info = data.commands_info.write();
                    *commands_info = help_text;
                }

                Ok(data)
            })
        })
        .options(options)
        .build();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let songbird = songbird::Songbird::serenity();
    let mut client = serenity::client::Client::builder(&config.token, intents)
        .framework(framework)
        .type_map_insert::<HttpKey>(HttpClient::new())
        .register_songbird_with(songbird.clone())
        .await
        .unwrap();

    start_queue_message_update(data_clone, songbird, (&client).into());

    start_deadlock_detection();

    client.start().await.unwrap()
}
