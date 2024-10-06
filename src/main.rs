use std::sync::Arc;

use queue::Data as QueueData;
use songbird::SerenityInit;

// YtDl requests need an HTTP client to operate -- we'll create and store our own.
use reqwest::Client as HttpClient;

use serenity::prelude::{GatewayIntents, TypeMapKey};

mod config;
use config::load_config;

mod commands;

mod events;

mod queue;

type Data = Arc<QueueData>;
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

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
            commands::queue(),
            commands::skip_to(),
            commands::skip(),
            commands::loop_song(),
            commands::loop_queue(),
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
                Ok(Data::default())
            })
        })
        .options(options)
        .build();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let client = serenity::client::Client::builder(&config.token, intents)
        .framework(framework)
        .type_map_insert::<HttpKey>(HttpClient::new())
        .register_songbird()
        .await;

    client.unwrap().start().await.unwrap()
}
