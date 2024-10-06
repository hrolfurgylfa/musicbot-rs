use songbird::TrackEvent;

use crate::{
    events::{TrackErrorNotifier, TrackStopHandler},
    get_songbird_manager,
    queue::AddSongResult,
    Context, Error,
};

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
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };
    let res = ctx.data().add_song(guild_id, ctx, play).await;
    match res {
        Ok(good_res) => match good_res {
            AddSongResult::NowPlaying(s) => ctx.say(format!("Playing \"{}\"", s.name)).await?,
            AddSongResult::AddedToQueue(s) => {
                ctx.say(format!("\"{}\" added to queue.", s.name)).await?
            }
        },
        Err(msg) => ctx.say(msg).await?,
    };

    Ok(())
}

/// Join a voice channel
#[poise::command(prefix_command, aliases("votes"), slash_command)]
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

    println!("Hello 0");
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
                TrackStopHandler::new(manager.clone(), ctx.data().clone(), guild_id),
            );
        }
        Err(e) => {
            println!("Faield to join channel: {:?}", e);
            ctx.say("Failed to join channel.").await?;
            return Err(Box::new(e));
        }
    }

    ctx.data().reset_queue(guild_id);
    ctx.say("Ready to play").await?;
    Ok(())
}

/// Leave the current voice channel
#[poise::command(prefix_command, slash_command)]
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

        ctx.data().reset_queue(guild_id);
        ctx.say("Left voice channel").await?;
    } else {
        ctx.say("Not in a voice channel").await?;
    }

    Ok(())
}

/// Show the current queue
#[poise::command(prefix_command, slash_command)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };
    let maybe_queue = ctx.data().get_queue(guild_id);
    if let Some(queue) = maybe_queue {
        let mut queue_str = "## Queue:\n```\n".to_owned();
        let current_id = queue.current.map(|q| q.index);
        for (i, song) in queue.songs.into_iter().enumerate() {
            if Some(i) == current_id {
                queue_str += &format!(
                    "{}. {} - {} (currently playing)\n",
                    i + 1,
                    song.name,
                    song.url
                );
            } else {
                queue_str += &format!("{}. {} - {}\n", i + 1, song.name, song.url);
            }
        }
        queue_str += "```";
        ctx.say(queue_str).await?;
    } else {
        ctx.say("The queue is empty.").await?;
    }

    Ok(())
}

/// Skip to a specific song in the queue
#[poise::command(prefix_command, slash_command)]
pub async fn skip_to(
    ctx: Context<'_>,
    #[description = "Index to skip to"] index: usize,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let res = ctx.data().play_index(guild_id, ctx, index - 1).await;
    match res {
        Ok(song) => ctx.say(format!("Skipping to \"{}\"", song.name)).await?,
        Err(err) => ctx.say(err).await?,
    };

    Ok(())
}

/// Skip over the current song
#[poise::command(prefix_command, slash_command)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    let res = ctx.data().play_index(guild_id, ctx, index - 1).await;
    match res {
        Ok(song) => ctx.say(format!("Skipping to \"{}\"", song.name)).await?,
        Err(err) => ctx.say(err).await?,
    };

    Ok(())
}

/// Loop on the current song forever
#[poise::command(prefix_command, slash_command, rename = "loop")]
pub async fn loop_song(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };
    let loop_ = ctx.data().set_loop_song(guild_id);
    if loop_ {
        ctx.say("Looping of the current song turned on.").await?;
    } else {
        ctx.say("Looping of the current song turned off.").await?;
    }

    Ok(())
}

/// Loop the queue when it finishes
#[poise::command(prefix_command, slash_command)]
pub async fn loop_queue(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };
    let loop_ = ctx.data().set_loop_queue(guild_id);
    if loop_ {
        ctx.say("Looping of the queue turned on.").await?;
    } else {
        ctx.say("Looping of the queue turned off.").await?;
    }

    Ok(())
}
