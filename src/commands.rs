use serenity::futures::future::join_all;
use songbird::{input::YoutubeDl, TrackEvent};

use crate::{
    events::TrackErrorNotifier,
    get_songbird_manager,
    typekeys::{HttpKey, SongTitleKey, SongUrlKey},
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
    #[description = "What to play"] url: String,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild().map(|g| g.id) else {
        ctx.say("This command is only supported in guilds.").await?;
        return Ok(());
    };

    // Some prepwork before gathering the data
    let do_search = !url.starts_with("http");
    let http_client = {
        let data = ctx.serenity_context().data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };

    // Fetch data about the selected video
    let mut src = if do_search {
        YoutubeDl::new_search(http_client, url.clone())
    } else {
        YoutubeDl::new(http_client, url.clone())
    };
    let mut aux_multiple = src
        .search(Some(1))
        .await
        .expect("Failed to get info about song.");
    if aux_multiple.len() == 0 {}
    let aux = aux_multiple.swap_remove(0);
    let title = aux.title.unwrap_or_else(|| "Unknown".to_owned());

    // Add the song to the queue
    {
        let songbird = get_songbird_manager(ctx).await;
        let Some(driver_lock) = songbird.get(guild_id) else {
            ctx.say("Not in voice channel, can't play.").await?;
            return Ok(());
        };
        let mut driver = driver_lock.lock().await;
        let handle = driver.enqueue(src.into()).await;
        let mut typemap = handle.typemap().write().await;
        typemap.insert::<SongTitleKey>(title.clone());
        typemap.insert::<SongUrlKey>(url);
    }

    ctx.say(format!("\"{}\" added to queue.", title)).await?;

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

    let songbird = get_songbird_manager(ctx).await;
    let Some(driver_lock) = songbird.get(guild_id) else {
        ctx.say("Not in a voice channel, no queue to show.").await?;
        return Ok(());
    };
    let driver = driver_lock.lock().await;
    if driver.queue().is_empty() {
        ctx.say("Queue is empty.").await?;
        return Ok(());
    }
    let current_uuid = driver.queue().current().map(|h| h.uuid());
    let queue = driver.queue().current_queue();
    let lines = queue.into_iter().enumerate().map(|(i, handle)| async move {
        let typemap = handle.typemap().read().await;
        let name = typemap
            .get::<SongTitleKey>()
            .map(|s| s.as_str())
            .unwrap_or("Unknown")
            .to_owned();
        let url = typemap
            .get::<SongUrlKey>()
            .map(|s| s.as_str())
            .unwrap_or("Unknown")
            .to_owned();
        if Some(handle.uuid()) == current_uuid {
            format!("{}. {} - {} (currently playing)", i + 1, name, url)
        } else {
            format!("{}. {} - {}", i + 1, name, url)
        }
    });
    let output = join_all(lines).await.join("\n");
    ctx.say(format!("## Queue:\n```\n{}\n```", output)).await?;

    Ok(())
}

/// Skip over the current song
#[poise::command(prefix_command, slash_command)]
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

    Ok(())
}
