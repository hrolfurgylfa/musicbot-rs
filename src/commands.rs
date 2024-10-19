use std::{process, sync::Arc, time::Duration};

use songbird::{input::YoutubeDl, tracks::Track, TrackEvent};

use tracing::{error, instrument};

use crate::{
    events::{TrackEndNotifier, TrackErrorNotifier},
    get_songbird_manager,
    playlist_info::{get_server_info, send_playlist_info, update_queue_messsage},
    typekeys::HttpKey,
    Context, Error, TrackData,
};

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

    let _defer = ctx.defer_or_broadcast().await?;

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
    let duration = aux.duration.unwrap_or(Duration::ZERO);

    // Add the song to the queue
    {
        let songbird = get_songbird_manager(ctx).await;
        let Some(driver_lock) = songbird.get(guild_id) else {
            ctx.say("Not in voice channel, can't play.").await?;
            return Ok(());
        };
        let mut driver = driver_lock.lock().await;
        let track = Track::new_with_data(
            src.into(),
            Arc::new(TrackData {
                title: title.clone(),
                url: aux.source_url,
                duration,
            }),
        );
        driver.enqueue(track).await;
    };

    ctx.say(format!("\"{}\" added to queue.", title)).await?;

    send_playlist_info(ctx, guild_id).await
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

    let _defer = ctx.defer_or_broadcast().await?;

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            ctx.say("Not in a voice channel").await?;
            return Ok(());
        }
    };

    let manager = get_songbird_manager(ctx).await;
    match manager.join(guild_id, connect_to).await {
        Ok(handler_lock) => {
            // Attach an event handler to see notifications of all track errors.
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
            handler.add_global_event(
                TrackEvent::End.into(),
                TrackEndNotifier::new(ctx.data().clone(), guild_id),
            );
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
#[poise::command(prefix_command, slash_command, guild_only, aliases("stop"))]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let manager = get_songbird_manager(ctx).await;
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            ctx.say(format!("Failed: {:?}", e)).await?;
        }

        ctx.say("Left voice channel").await?;

        // Unset the update, since the bot is no longer in the VC
        let prev_status_message_loc = {
            let server_info_lock = get_server_info(ctx.data().clone(), guild_id).await;
            let mut server_info = server_info_lock.lock();
            let prev = server_info.status_message;
            server_info.status_message = None;
            prev
        };

        // Do one final update to show that the queue is now empty
        if let Some(loc) = prev_status_message_loc {
            update_queue_messsage(ctx.data().clone(), manager, &(&ctx).into(), guild_id, loc).await;
        }
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

    send_playlist_info(ctx, guild_id).await
}

/// Skip over the current song
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let songbird = get_songbird_manager(ctx).await;
    let Some(driver_lock) = songbird.get(guild_id) else {
        ctx.say("No playing anything, can't skip.").await?;
        return Ok(());
    };
    let driver = driver_lock.lock().await;
    driver.queue().skip()?;
    ctx.say("Skipping to the next song.").await?;

    send_playlist_info(ctx, guild_id).await
}

/// Restarts the bot, use when it freezes
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn restart(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Restarting...").await?;

    let err = {
        let mut args_iter = std::env::args_os().into_iter();
        let path = args_iter
            .next()
            .expect("Program not run with any program arg. Cannot restart.");
        let args: Vec<_> = args_iter.collect();

        use std::os::unix::process::CommandExt;
        let err = process::Command::new(path).args(args).exec();
        error!(?err, "Failed to restart process: {:?}", err);
        err
    };
    ctx.say(format!("Failed to restart: {}", err)).await?;

    Ok(())
}
