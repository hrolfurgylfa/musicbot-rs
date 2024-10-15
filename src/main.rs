use std::{env, sync::Arc};

use tracing::{error, instrument};

use reqwest::Client as HttpClient;

use serenity::{
    async_trait,
    prelude::{GatewayIntents, TypeMapKey},
};
use songbird::{
    input::YoutubeDl, tracks::Track, Event, EventContext, EventHandler as VoiceEventHandler,
    SerenityInit, TrackEvent,
};

use tracing_appender;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Layer, Registry};

pub struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

pub struct TrackErrorNotifier;

#[async_trait]
impl VoiceEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                tracing::error!(
                    ?handle,
                    ?state,
                    "Track \"{}\" encountered an error.",
                    handle.uuid()
                );
            }
        }

        None
    }
}

#[derive(Debug)]
struct Data;
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Arc<Data>, Error>;

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
            tracing::error!(?error, "Unknown error: \"{:?}\":", error);
        }
    }
}

#[tokio::main]
async fn main() {
    let data = Arc::new(Data);
    let token = env::var("TOKEN").expect("Bot token not found.");

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
                .with_filter(EnvFilter::new("info,music_bot=debug,songbird=trace")),
        )
        .with(console_subscriber::spawn());
    tracing::subscriber::set_global_default(subscriber).expect("unable to set global subscriber");

    let options = poise::FrameworkOptions {
        commands: vec![help(), join(), leave(), play(), queue()],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("-".to_owned()),
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

                Ok(data)
            })
        })
        .options(options)
        .build();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let songbird = songbird::Songbird::serenity();
    let mut client = serenity::client::Client::builder(&token, intents)
        .framework(framework)
        .type_map_insert::<HttpKey>(HttpClient::new())
        .register_songbird_with(songbird.clone())
        .await
        .unwrap();

    client.start().await.unwrap()
}

/// Show the help menu
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration {
            extra_text_at_bottom: "This is an example bot made to showcase features of my custom Discord bot framework",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

/// Play a song or search YouTube for a song
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "What to play"] play: String,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    // Some prepwork before gathering the data
    let do_search = !play.starts_with("http");
    let http_client = {
        let data = ctx.serenity_context().data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };

    // Fetch data about the selected video
    let mut src = if do_search {
        YoutubeDl::new_search(http_client, play.clone())
    } else {
        YoutubeDl::new(http_client, play.clone())
    };
    let mut aux_multiple = src
        .search(Some(1))
        .await
        .expect("Failed to get info about song.")
        .collect::<Vec<_>>();
    if aux_multiple.len() == 0 {}
    let aux = aux_multiple.swap_remove(0);
    let title = aux.title.unwrap_or_else(|| "Unknown".to_owned());

    // Add the song to the queue
    {
        let songbird = songbird::get(ctx.serenity_context())
            .await
            .expect("Songbird Voice client placed in at initialisation.")
            .clone();
        let Some(driver_lock) = songbird.get(guild_id) else {
            ctx.say("Not in voice channel, can't play.").await?;
            return Ok(());
        };
        let mut driver = driver_lock.lock().await;
        let track = Track::new(src.into());
        driver.enqueue(track).await;
    };

    ctx.say(format!("\"{}\" added to queue.", title)).await?;

    Ok(())
}

/// Join a voice channel
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn join(
    ctx: Context<'_>,
    // #[description = "Choice to retrieve votes for"] voice_channel: Option<VoiceState>,
) -> Result<(), Error> {
    let (guild_id, channel_id) = {
        let Some(guild) = ctx.guild() else {
            ctx.say("This command is only supported in guilds.").await?;
            return Ok(());
        };

        let channel_id = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);
        (guild.id, channel_id)
    };

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            ctx.say("Not in a voice channel").await?;
            return Ok(());
        }
    };

    let songbird = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    match songbird.join(guild_id, connect_to).await {
        Ok(handler_lock) => {
            // Attach an event handler to see notifications of all track errors.
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
        }
        Err(e) => {
            error!("Faield to join channel: {:?}", e);
            ctx.say("Failed to join channel.").await?;
            return Err(Box::new(e));
        }
    }

    ctx.say("Ready to play").await?;
    Ok(())
}

/// Leave the current voice channel
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let songbird = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let has_handler = songbird.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = songbird.remove(guild_id).await {
            ctx.say(format!("Failed: {:?}", e)).await?;
        }

        ctx.say("Left voice channel").await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }

    Ok(())
}

/// Show the current queue
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let songbird = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let Some(call_lock) = songbird.get(guild_id) else {
        ctx.say("No call available.").await?;
        return Ok(());
    };

    let message = {
        let call = call_lock.lock().await;
        if let Some(track) = call.queue().current() {
            match track.get_info().await {
                Ok(a) => format!("Time: {:?}", a.position),
                Err(e) => format!("Error getting info: {:?}", e),
            }
        } else {
            "No current track.".to_owned()
        }
    };

    ctx.say(message).await?;

    Ok(())
}
