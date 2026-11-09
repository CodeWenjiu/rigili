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

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerStatus {
    pub phase: PlaybackPhase,
    pub paused: bool,
    pub time_pos: f64,
    pub duration: f64,
    pub volume: f64,
    pub speed: f64,
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
            speed: 1.0,
            hwdec: None,
            audio_codec: None,
            video_size: None,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PlaybackPhase, PlayerStatus};

    #[test]
    fn starts_idle() {
        let status = PlayerStatus::default();
        assert_eq!(status.phase, PlaybackPhase::Idle);
        assert!(!status.paused);
    }
}

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}
