use crate::VideoSize;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerStatus {
    pub idle: bool,
    pub paused: bool,
    pub loading: bool,
    pub ended: bool,
    pub buffering: bool,
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
            idle: true,
            paused: false,
            loading: false,
            ended: false,
            buffering: false,
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

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}
