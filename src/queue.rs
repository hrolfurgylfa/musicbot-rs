use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use parking_lot::{MappedRwLockWriteGuard, RwLock, RwLockWriteGuard};
use serenity::all::GuildId;
use songbird::{input::YoutubeDl, tracks::TrackHandle, Songbird};

use crate::{get_songbird_manager, Context, HttpKey};

#[derive(Debug, Default)]
pub struct Data {
    server_queues: RwLock<HashMap<GuildId, ServerQueue>>,
}

#[derive(Debug, Clone, Default)]
pub struct ServerQueue {
    loop_song: bool,
    loop_queue: bool,
    pub current: Option<CurrentSong>,
    pub songs: Vec<Song>,
}

#[derive(Debug, Clone)]
pub struct CurrentSong {
    pub index: usize,
    handle: TrackHandle,
}

#[derive(Debug, Clone)]
pub struct Song {
    pub name: String,
    pub url: String,
    src: YoutubeDl,
}

#[derive(Debug, Clone)]
pub enum AddSongResult {
    NowPlaying(Song),
    AddedToQueue(Song),
}

async fn get_song_data(ctx: Context<'_>, url: String) -> Result<Song, String> {
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

    return Ok(Song {
        name: title,
        url: aux
            .source_url
            .or_else(|| if !do_search { Some(url) } else { None })
            .unwrap_or_else(|| "Unknown".to_owned()),
        src,
    });
}

impl Data {
    fn get_mut_queue_or_default(&self, guild_id: GuildId) -> MappedRwLockWriteGuard<ServerQueue> {
        let queues = self.server_queues.write();
        RwLockWriteGuard::map(queues, |queues| {
            let queue = match queues.entry(guild_id) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(ServerQueue::default()),
            };
            queue
        })
    }

    pub fn get_queue(&self, guild_id: GuildId) -> Option<ServerQueue> {
        let queues = self.server_queues.read();
        queues.get(&guild_id).map(|sq| sq.clone())
    }

    pub fn reset_queue(&self, guild_id: GuildId) -> Option<ServerQueue> {
        self.server_queues.write().remove(&guild_id)
    }

    pub async fn add_song(
        &self,
        guild_id: GuildId,
        ctx: Context<'_>,
        url: String,
    ) -> Result<AddSongResult, String> {
        // Make sure we're actually in a voice channel

        let song = get_song_data(ctx, url).await?;

        // Add the data we got into the queue
        let (loc, song_if_not_first) = {
            let mut queue = self.get_mut_queue_or_default(guild_id);
            if queue.current.is_some() {
                queue.songs.push(song.clone());
                (queue.songs.len() - 1, Some(song))
            } else {
                queue.songs.push(song);
                (queue.songs.len() - 1, None)
            }
        };

        let result = if let Some(song) = song_if_not_first {
            AddSongResult::AddedToQueue(song)
        } else {
            let manager = get_songbird_manager(ctx).await;
            let playing = self.play_index_unchecked(manager, guild_id, loc).await?;
            AddSongResult::NowPlaying(playing)
        };
        Ok(result)
    }

    pub async fn next_song(
        &self,
        songbird: Arc<Songbird>,
        guild_id: GuildId,
        pause_current: bool,
    ) -> Result<Option<Song>, String> {
        let index_to_play = {
            let mut queue = self.get_mut_queue_or_default(guild_id);
            let songs_len = queue.songs.len();
            let Some(current) = &queue.current else {
                return Err("No curernt song, can't play the next one.".to_owned());
            };

            let mut new_index = current.index + (if queue.loop_song { 0 } else { 1 });
            if queue.loop_queue {
                new_index = new_index % songs_len;
            }

            if new_index >= songs_len {
                queue.current = None;
                None
            } else {
                Some(new_index)
            }
        };

        if let Some(new_index) = index_to_play {
            self.play_index_unchecked(songbird, guild_id, new_index)
                .await
                .map(|s| Some(s))
        } else {
            Ok(None)
        }
    }

    pub async fn play_index(
        &self,
        guild_id: GuildId,
        ctx: Context<'_>,
        index: usize,
    ) -> Result<Song, String> {
        {
            // Get the current servers queue
            let queues = self.server_queues.read();
            let Some(queue) = queues.get(&guild_id) else {
                return Err("Nothing playing, can't jump to index.".to_owned());
            };

            // Make sure the index fits in the queue
            let Some(_) = queue.songs.get(index) else {
                return Err(format!(
                    "Can't find song with index {} in queue.",
                    index + 1
                ));
            };

            // Stop the current song if a song is playing
            if let Some(current) = &queue.current {
                current
                    .handle
                    .pause()
                    .map_err(|e| format!("Failed to pause previous track: {}", e))?;
            }
        }

        // Play the song
        let songbird = get_songbird_manager(ctx).await;
        self.play_index_unchecked(songbird, guild_id, index).await
    }

    async fn play_index_unchecked(
        &self,
        songbird: Arc<songbird::Songbird>,
        guild_id: GuildId,
        index: usize,
    ) -> Result<Song, String> {
        let handler_lock = songbird
            .get(guild_id)
            .ok_or_else(|| format!("Not in a voice channel to play in"))?;
        let mut handler = handler_lock.lock().await;
        let mut queue = self.get_mut_queue_or_default(guild_id);
        let song = queue
            .songs
            .get_mut(index)
            .ok_or_else(|| format!("Song {} not found.", index + 1))?
            .clone();
        let handle = handler.play_input(song.src.clone().into());
        queue.current = Some(CurrentSong { index, handle });

        Ok(song)
    }

    pub fn set_loop_song(&self, guild_id: GuildId) -> bool {
        let mut queue = self.get_mut_queue_or_default(guild_id);
        queue.loop_song = !queue.loop_song;
        queue.loop_song
    }

    pub fn set_loop_queue(&self, guild_id: GuildId) -> bool {
        let mut queue = self.get_mut_queue_or_default(guild_id);
        queue.loop_queue = !queue.loop_queue;
        queue.loop_queue
    }
}
