use songbird::{input::YoutubeDl, TrackEvent};

use crate::{Context, Error, HttpKey, TrackErrorNotifier};

/// Show this help menu
#[poise::command(prefix_command, slash_command)]
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
#[poise::command(prefix_command, slash_command)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "What to play"] play: String,
) -> Result<(), Error> {
    let do_search = !play.starts_with("http");

    let maybe_guild_id = match ctx.guild() {
        Some(a) => Some(a.id),
        None => None,
    };
    let guild_id = match maybe_guild_id {
        Some(t) => t,
        None => {
            ctx.say("This command is only supported in guilds.").await?;
            return Ok(());
        }
    };

    let http_client = {
        let data = ctx.serenity_context().data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };

    let manager = ctx.data().songbird.clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let src = if do_search {
            YoutubeDl::new_search(http_client, play)
        } else {
            YoutubeDl::new(http_client, play)
        };
        let _ = handler.play_input(src.clone().into());

        ctx.say("Playing song").await?;
    } else {
        ctx.say("Not in a voice channel to play in").await?;
    }

    Ok(())
}

/// Join a voice channel
#[poise::command(prefix_command, track_edits, aliases("votes"), slash_command)]
pub async fn join(
    ctx: Context<'_>,
    // #[description = "Choice to retrieve votes for"] voice_channel: Option<VoiceState>,
) -> Result<(), Error> {
    let (guild_id, channel_id) = {
        let maybe_guild = ctx.guild();
        let guild = match maybe_guild {
            Some(a) => a,
            None => {
                drop(maybe_guild);
                ctx.say("This command is only supported in guilds.").await?;
                return Ok(());
            }
        };
        let channel_id = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);
        (guild.id, channel_id)
    };

    println!("Hello 0");
    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            ctx.say("Not in a voice channel").await?;
            return Ok(());
        }
    };

    let manager = ctx.data().songbird.clone();

    match manager.join(guild_id, connect_to).await {
        Ok(handler_lock) => {
            // Attach an event handler to see notifications of all track errors.
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
        }
        Err(e) => {
            println!("Faield to join channel: {:?}", e);
            ctx.say("Failed to join channel.").await?;
            return Err(Box::new(e));
        }
    }

    ctx.say("Ready to play").await?;
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let maybe_guild_id = match ctx.guild() {
        Some(a) => Some(a.id),
        None => None,
    };
    let guild_id = match maybe_guild_id {
        Some(t) => t,
        None => {
            ctx.say("This command is only supported in guilds.").await?;
            return Ok(());
        }
    };

    let manager = ctx.data().songbird.clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            ctx.say(format!("Failed: {:?}", e)).await?;
        }

        ctx.say("Left voice channel").await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }

    Ok(())
}
