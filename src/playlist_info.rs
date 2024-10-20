use std::fmt::Write;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use poise::{send_reply, CreateReply};
use serenity::all::{Color, CreateEmbed, EditMessage};
use serenity::{all::GuildId, futures::future::join_all};
use songbird::Songbird;

use parking_lot::Mutex;
use tokio::time::{self, sleep, Instant};
use tracing::{error, instrument, trace, warn};

use crate::serenity_query::SerenityQuery;
use crate::trimmed_embed::{Size, TrimmedEmbed};
use crate::{get_songbird_manager, Context, Data, Error, ServerInfo, Song};
use crate::{MsgLocation, TrackData};

pub async fn get_server_info(data: Arc<Data>, guild_id: GuildId) -> Arc<Mutex<ServerInfo>> {
    let map = data.server_info.read();

    if let Some(a) = map.get(&guild_id) {
        a.clone()
    } else {
        drop(map);
        let mut write_map = data.server_info.write();
        write_map
            .entry(guild_id)
            .or_insert_with(|| Arc::new(Mutex::new(ServerInfo::default())))
            .clone()
    }
}

fn clean_song_title(title: impl AsRef<str>) -> String {
    let str_title = title.as_ref();
    str_title.replace("[", "(").replace("]", ")")
}

fn build_previously_played<'a>(previously_played: impl Iterator<Item = &'a Song>) -> String {
    let mut str = "### Previously Played\n".to_owned();

    for song in previously_played {
        let title = clean_song_title(&song.title);
        if let Some(url) = &song.url {
            write!(str, "* [{}]({})\n", title, url).unwrap();
        } else {
            write!(str, "* {}\n", title).unwrap();
        }
    }

    str
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs() % 60;
    let minutes = (duration.as_secs() / 60) % 60;
    let hours = (duration.as_secs() / 60) / 60;
    if hours > 0 {
        format!("{}:{:0>2}:{:0>2}", hours, minutes, seconds)
    } else {
        format!("{}:{:0>2}", minutes, seconds)
    }
}

enum NowPlayingResult {
    NotInChannel,
    NotPlaying,
    Playing(String),
}

/// Try calling a function that likes hanging forever.
///
/// Tries 3 times and gives it 1 second each time, retunrs None if all 3 attempts hang.
async fn try_call_hanging<T, F, Ft>(func: F) -> Option<T>
where
    F: Fn() -> Ft,
    Ft: Future<Output = T>,
{
    for _ in 0..3 {
        tokio::select! {
            track_state = func() => {return Some(track_state);}
            _ = sleep(Duration::from_secs(1)) => {}
        }
    }

    None
}

#[instrument]
async fn build_now_playing(songbird: Arc<Songbird>, guild_id: GuildId) -> NowPlayingResult {
    let Some(driver_lock) = songbird.get(guild_id) else {
        return NowPlayingResult::NotInChannel;
    };
    let driver = driver_lock.lock().await;
    if driver.queue().is_empty() {
        return NowPlayingResult::NotPlaying;
    }
    let Some(current) = driver.queue().current() else {
        return NowPlayingResult::NotPlaying;
    };

    let mut str = "### Now Playing\n".to_owned();
    {
        let data = current.data::<TrackData>();
        let length = format_duration(data.duration);
        let state = try_call_hanging(|| async {
            current
                .get_info()
                .await
                .expect("Failed to get track state.")
        })
        .await
        .expect("Failed to call get_info");
        let pos = { format_duration(state.position) };

        if let Some(url) = &data.url {
            write!(str, "[{}]({})\n[ {} / {} ]\n", data.title, url, pos, length).unwrap();
        } else {
            write!(str, "{}\n[ {} / {} ]\n", data.title, pos, length).unwrap();
        }
    }

    {
        let queue = driver.queue().current_queue();
        if queue.len() > 1 {
            str += "\n### Up Next:\n";
        }
        let up_next_lines = queue
            .iter()
            .skip(1)
            .enumerate()
            .map(|(i, handle)| async move {
                let data = handle.data::<TrackData>();

                if let Some(url) = &data.url {
                    format!("{}. [{}]({})\n", i + 1, data.title, url)
                } else {
                    format!("{}. {}\n", i + 1, data.title)
                }
            });
        for line in join_all(up_next_lines).await {
            str += &line
        }
    }

    NowPlayingResult::Playing(str)
}

#[instrument(skip(songbird, query))]
async fn get_playlist_info_embeds(
    data: Arc<Data>,
    songbird: Arc<Songbird>,
    query: &SerenityQuery,
    guild_id: GuildId,
) -> (Vec<CreateEmbed>, bool) {
    let server_info_lock = get_server_info(data.clone(), guild_id).await;

    let previously_played_text = {
        let server_info = server_info_lock.lock();
        let previously_played_text = build_previously_played(server_info.previous_songs.iter());

        previously_played_text
    };

    let (now_playing_text, not_in_channel) = match build_now_playing(songbird, guild_id).await {
        NowPlayingResult::NotInChannel => ("### Nothing playing".to_owned(), true),
        NowPlayingResult::NotPlaying => ("### Nothing playing".to_owned(), false),
        NowPlayingResult::Playing(text) => (text, false),
    };

    let radio_name = query
        .get_guild_name(guild_id)
        .await
        .unwrap_or_else(|| "Hroi".to_owned());
    let help_text = {
        let commands_info = data.commands_info.read();
        format!(
            "### Welcome to the {}-Radio!\n\n**Controls:**\n```\n{}```\n",
            radio_name, commands_info
        )
    };

    let mut size = Size::new();
    let embeds = vec![
        TrimmedEmbed::new(&mut size)
            .description(help_text)
            .color(Color::DARK_GREEN)
            .into(),
        TrimmedEmbed::new(&mut size)
            .too_big_msg("...")
            .truncate_description_newline()
            .description(previously_played_text)
            .color(Color::DARK_PURPLE)
            .into(),
        TrimmedEmbed::new(&mut size)
            .too_big_msg("...")
            .truncate_description_newline()
            .description(now_playing_text)
            .color(Color::BLUE)
            .into(),
    ];

    (embeds, not_in_channel)
}

pub async fn update_queue_messsage(
    data: Arc<Data>,
    songbird: Arc<Songbird>,
    query: &SerenityQuery,
    guild_id: GuildId,
    loc: MsgLocation,
) -> bool {
    let (embeds, not_in_channel) = get_playlist_info_embeds(data, songbird, query, guild_id).await;

    let res = loc
        .channel_id
        .edit_message(query, loc.message_id, EditMessage::new().embeds(embeds))
        .await;
    match res {
        // We only want to continue if the bot is in a voice channel
        Ok(_) => !not_in_channel,
        Err(e) => {
            warn!(?guild_id, ?loc, err = %e, "Lost track of my queue message.");
            false
        }
    }
}

#[instrument(skip(ctx))]
pub async fn send_playlist_info(ctx: Context<'_>, guild_id: GuildId) -> Result<(), Error> {
    let songbird = get_songbird_manager(ctx).await;
    let query: SerenityQuery = (&ctx).into();
    let (embeds, not_in_channel) =
        get_playlist_info_embeds(ctx.data().clone(), songbird, &query, guild_id).await;

    let mut reply = CreateReply::default();
    reply.embeds = embeds;
    let reply_handle = send_reply(ctx, reply).await?;

    // Set the status message so the message gets auto updated
    if !not_in_channel {
        let message = reply_handle.message().await?;
        let loc = MsgLocation::new(message.channel_id, message.id);

        let server_info_lock = get_server_info(ctx.data().clone(), guild_id).await;
        let mut server_info = server_info_lock.lock();
        server_info.status_message = Some(loc);
    }
    Ok(())
}

async fn run_message_updates(data: &Arc<Data>, songbird: &Arc<Songbird>, query: &SerenityQuery) {
    let to_update = {
        let server_infos = data.server_info.read();
        let mut to_update = Vec::with_capacity(server_infos.len());
        for (guild_id, server_info_lock) in server_infos.iter() {
            let server_info = server_info_lock.lock();
            if let Some(status_msg) = server_info.status_message {
                to_update.push((*guild_id, status_msg, server_info_lock.clone()));
            }
        }
        to_update
    };
    let to_update_str = if to_update.len() > 0 {
        let mut guild_ids_str = String::with_capacity(100);
        for (guild_id, _, _) in to_update.iter() {
            write!(guild_ids_str, "{}, ", guild_id).unwrap();
        }
        guild_ids_str.truncate(guild_ids_str.len() - 2);
        guild_ids_str
    } else {
        "".to_owned()
    };
    trace!("Updating queue message for guilds: [{}]", to_update_str);
    for (guild_id, status_msg_loc, server_info_lock) in to_update {
        let success = update_queue_messsage(
            data.clone(),
            songbird.clone(),
            &query,
            guild_id,
            status_msg_loc,
        )
        .await;
        if !success {
            let mut server_info = server_info_lock.lock();
            server_info.status_message = None;
        }
    }
}

pub fn start_queue_message_update(data: Arc<Data>, songbird: Arc<Songbird>, query: SerenityQuery) {
    tokio::spawn(async move {
        loop {
            let start = Instant::now();
            tokio::select! {
                _ = run_message_updates(&data, &songbird, &query) => {}
                _ = sleep(Duration::from_secs(5)) => {
                    error!("run_message_updates took more than 5 seconds. Cancelling and trying again.");
                }
            }

            let end = Instant::now();
            let sleep_time = Duration::from_secs(2)
                .saturating_sub(end - start)
                .min(Duration::from_millis(1500));
            time::sleep(sleep_time).await;
        }
    });
}
