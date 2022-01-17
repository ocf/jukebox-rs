use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IncomingMessage {
    AddSong { url: String },
    DelSong { position: usize },
    Volume { volume: usize },
    Skip,
    Ping,
    Pause,
    Resume,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OutgoingMessage {
    NewState {
        song: Song,
        user: String,
        queues: HashMap<String, Vec<Song>>,
        volume: usize,
        usernames: Vec<String>,
    },
    Pong,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Song {
    title: String,
    url: String,
    thumbnail: String,
    webpage_url: String,
}
