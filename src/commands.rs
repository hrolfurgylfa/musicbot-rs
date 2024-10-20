use std::{fmt::Write, process, sync::Arc, time::Duration};

use serde::Deserialize;
use songbird::{input::YoutubeDl, tracks::Track, CoreEvent, TrackEvent};

use tokio::process::Command as TokioCommand;
use tracing::{error, instrument, warn};

use crate::{
    events::{TrackDisconnectNotifier, TrackEndNotifier, TrackErrorNotifier},
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

async fn get_single_track<'a>(mut src: YoutubeDl<'static>) -> Track {
    let mut aux_multiple = src
        .search(Some(1))
        .await
        .expect("Failed to get info about song.")
        .collect::<Vec<_>>();
    if aux_multiple.len() == 0 {}
    let aux = aux_multiple.swap_remove(0);
    let title = aux.title.unwrap_or_else(|| "Unknown".to_owned());
    let duration = aux.duration.unwrap_or(Duration::ZERO);

    Track::new_with_data(
        src.into(),
        Arc::new(TrackData {
            title,
            url: aux.source_url,
            duration,
        }),
    )
}

#[derive(Deserialize)]
struct YtDlpFlatPlaylistOutput {
    title: String,
    url: String,
    duration: f32,
}

async fn get_multiple_tracks(client: reqwest::Client, url: &str) -> Result<Vec<Track>, String> {
    let output = TokioCommand::new("yt-dlp")
        .arg("-j")
        .arg(&url)
        .arg("--flat-playlist")
        .output()
        .await
        .expect("Could not find yt-dlp on path");
    if !output.status.success() {
        panic!(
            "yt-dlp failed with non-zero status code: {}",
            std::str::from_utf8(&output.stderr[..]).unwrap_or("<no error message>")
        );
    }
    let out = output
        .stdout
        .split(|&b| b == b'\n')
        .filter(|&x| (!x.is_empty()))
        .take(50)
        .map(serde_json::from_slice)
        .collect::<Result<Vec<YtDlpFlatPlaylistOutput>, _>>()
        .map_err(|e| {
            warn!("Failed to parse playlist: {:?}", e);
            "Failed to request playlist, are you sure what you provided is a playlist?".to_owned()
        })?
        .into_iter()
        .map(|output| {
            Track::new_with_data(
                YoutubeDl::new(client.clone(), output.url.clone()).into(),
                Arc::new(TrackData {
                    title: output.title,
                    duration: Duration::from_secs_f32(output.duration),
                    url: Some(output.url.clone()),
                }),
            )
        })
        .collect::<Vec<Track>>();

    Ok(out)
}

/// Play a song or search YouTube for a song
#[instrument]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "What to play"] play: String,
    #[description = "Put the full playlist onto the queue?"]
    #[flag]
    playlist: bool,
) -> Result<(), Error> {
    let _defer = ctx.defer_or_broadcast().await?;

    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let songbird = get_songbird_manager(ctx).await;
    let Some(driver_lock) = songbird.get(guild_id) else {
        ctx.say("Not in voice channel, can't play.").await?;
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
    let tracks = if do_search {
        let src = YoutubeDl::new_search(http_client, play);
        vec![get_single_track(src).await]
    } else {
        if playlist {
            match get_multiple_tracks(http_client, &play).await {
                Ok(ok) => ok,
                Err(err) => {
                    ctx.say(err).await?;
                    return Ok(());
                }
            }
        } else {
            let src = YoutubeDl::new(http_client, play);
            vec![get_single_track(src).await]
        }
    };

    // Add the song to the queue
    let mut songs_added = vec![];
    {
        let mut driver = driver_lock.lock().await;
        for track in tracks {
            let data = track.user_data.downcast_ref::<TrackData>().unwrap();
            let preload_time = data.duration.saturating_sub(Duration::from_secs(5));
            songs_added.push(data.title.clone());
            driver.enqueue_with_preload(track, Some(preload_time));
        }
    }

    // Make the list of songs added for discord
    let mut songs_added_str = format!("\"{}\"", songs_added[0]);
    if songs_added.len() > 1 {
        for title in songs_added.iter().skip(1).take(songs_added.len() - 2) {
            write!(songs_added_str, ", \"{}\"", title).unwrap();
        }
        write!(songs_added_str, " and \"{}\"", songs_added.last().unwrap()).unwrap();
    }
    ctx.say(format!("{} added to queue.", songs_added_str))
        .await?;

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
            handler.add_global_event(
                CoreEvent::DriverDisconnect.into(),
                TrackDisconnectNotifier::new(manager.clone(), guild_id),
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
    {
        let driver = driver_lock.lock().await;
        driver.queue().skip()?;
    }
    ctx.say("Skipping to the next song.").await?;

    send_playlist_info(ctx, guild_id).await?;

    Ok(())
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
