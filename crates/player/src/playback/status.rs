use std::sync::{Arc, Mutex};

use crate::VideoSize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackPhase {
    #[default]
    Idle,
    Loading,
    Buffering,
    Playing,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BufferedRange {
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerStatus {
    pub phase: PlaybackPhase,
    pub paused: bool,
    pub time_pos: f64,
    pub duration: f64,
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
    pub buffered: Vec<BufferedRange>,
    pub hwdec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_size: Option<VideoSize>,
    pub error: Option<String>,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            phase: PlaybackPhase::Idle,
            paused: false,
            time_pos: 0.0,
            duration: 0.0,
            volume: 100.0,
            muted: false,
            speed: 1.0,
            buffered: Vec::new(),
            hwdec: None,
            audio_codec: None,
            video_size: None,
            error: None,
        }
    }
}

pub(super) type StatusSlot = Mutex<Arc<PlayerStatus>>;

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}
