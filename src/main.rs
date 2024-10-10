use std::sync::Arc;

use reqwest::Client as HttpClient;

use serenity::{all::Http, prelude::GatewayIntents};
use songbird::SerenityInit;

use tracing::level_filters::LevelFilter;
use tracing_appender;
use tracing_subscriber::{layer::SubscriberExt, Layer, Registry};

mod config;
use config::{load_config, Config};

mod commands;

mod events;

mod trimmed_embed;

mod typekeys;
use typekeys::HttpKey;

mod tracing_webhook;

#[derive(Debug, Clone)]
struct Data {
    config: Config,
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

async fn get_songbird_manager(ctx: Context<'_>) -> Arc<songbird::Songbird> {
    songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone()
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
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
    let config_clone = config.clone();
    let http = Http::new(&config.token);

    // Setup logging
    let file_appender = tracing_appender::rolling::daily("./logs", "music_bot.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = Registry::default()
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(file_writer)
                .with_filter(LevelFilter::DEBUG),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(std::io::stdout)
                .with_filter(LevelFilter::INFO),
        )
        .with(tracing_webhook::Layer::build(config.clone(), http));
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
            prefix: Some("=".to_owned()),
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
                Ok(Data { config })
            })
        })
        .options(options)
        .build();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let client = serenity::client::Client::builder(&config_clone.token, intents)
        .framework(framework)
        .type_map_insert::<HttpKey>(HttpClient::new())
        .register_songbird()
        .await;

    client.unwrap().start().await.unwrap()
}
