use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Popular {
    #[serde(default)]
    pub list: Vec<PopularVideo>,
    #[serde(default)]
    pub no_more: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PopularVideo {
    pub aid: u64,
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    pub pic: String,
    pub title: String,
    pub duration: u64,
    pub pubdate: i64,
    pub owner: Owner,
    #[serde(default)]
    pub stat: Stat,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct VideoInfo {
    pub bvid: String,
    pub aid: u64,
    pub cid: u64,
    pub pic: String,
    pub title: String,
    #[serde(default)]
    pub desc: String,
    pub duration: u64,
    pub pubdate: i64,
    pub owner: Owner,
    #[serde(default)]
    pub stat: Stat,
    #[serde(default)]
    pub pages: Vec<VideoPage>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct VideoPage {
    pub cid: u64,
    pub page: u32,
    #[serde(default)]
    pub part: String,
    pub duration: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PlayUrl {
    #[serde(default)]
    pub dash: Option<Dash>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Dash {
    #[serde(default)]
    pub video: Vec<DashVideo>,
    #[serde(default)]
    pub audio: Vec<DashAudio>,
}

impl Dash {
    pub fn best_video(&self) -> Option<&DashVideo> {
        self.video
            .iter()
            .filter(|video| video.codecid == 7)
            .max_by_key(|video| video.bandwidth)
            .or_else(|| self.video.iter().max_by_key(|video| video.bandwidth))
    }

    pub fn best_audio(&self) -> Option<&DashAudio> {
        self.audio.iter().max_by_key(|audio| audio.bandwidth)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawDashVideo")]
pub struct DashVideo {
    pub id: u32,
    pub base_url: String,
    pub bandwidth: u64,
    pub width: u32,
    pub height: u32,
    pub codecs: String,
    pub codecid: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawDashAudio")]
pub struct DashAudio {
    pub id: u32,
    pub base_url: String,
    pub bandwidth: u64,
    pub codecs: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDashVideo {
    id: u32,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default, rename = "base_url")]
    base_url_snake: Option<String>,
    bandwidth: u64,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    codecs: String,
    #[serde(default)]
    codecid: u32,
}

impl From<RawDashVideo> for DashVideo {
    fn from(raw: RawDashVideo) -> Self {
        Self {
            id: raw.id,
            base_url: raw.base_url.or(raw.base_url_snake).unwrap_or_default(),
            bandwidth: raw.bandwidth,
            width: raw.width,
            height: raw.height,
            codecs: raw.codecs,
            codecid: raw.codecid,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDashAudio {
    id: u32,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default, rename = "base_url")]
    base_url_snake: Option<String>,
    bandwidth: u64,
    #[serde(default)]
    codecs: String,
}

impl From<RawDashAudio> for DashAudio {
    fn from(raw: RawDashAudio) -> Self {
        Self {
            id: raw.id,
            base_url: raw.base_url.or(raw.base_url_snake).unwrap_or_default(),
            bandwidth: raw.bandwidth,
            codecs: raw.codecs,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct QrCode {
    pub url: String,
    pub qrcode_key: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct QrPoll {
    pub code: u32,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Nav {
    #[serde(default)]
    pub is_login: bool,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub face: String,
    #[serde(default)]
    pub mid: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Owner {
    pub mid: u64,
    pub name: String,
    #[serde(default)]
    pub face: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Stat {
    #[serde(default)]
    pub view: u64,
    #[serde(default)]
    pub danmaku: u64,
    #[serde(default)]
    pub like: u64,
}

#[cfg(test)]
mod tests {
    use super::{Dash, DashAudio, DashVideo};

    fn video(codecid: u32, bandwidth: u64) -> DashVideo {
        DashVideo {
            id: 32,
            base_url: format!("https://example.invalid/{codecid}.m4s"),
            bandwidth,
            width: 852,
            height: 480,
            codecs: String::new(),
            codecid,
        }
    }

    fn audio(bandwidth: u64) -> DashAudio {
        DashAudio {
            id: 30280,
            base_url: "https://example.invalid/audio.m4s".into(),
            bandwidth,
            codecs: String::new(),
        }
    }

    #[test]
    fn best_video_prefers_avc_over_higher_bandwidth_codecs() {
        let dash = Dash {
            video: vec![video(13, 900_000), video(7, 600_000)],
            audio: Vec::new(),
        };
        assert_eq!(dash.best_video().map(|video| video.codecid), Some(7));
    }

    #[test]
    fn best_video_falls_back_to_highest_bandwidth() {
        let dash = Dash {
            video: vec![video(12, 600_000), video(13, 900_000)],
            audio: Vec::new(),
        };
        assert_eq!(dash.best_video().map(|video| video.codecid), Some(13));
    }

    #[test]
    fn best_audio_picks_highest_bandwidth() {
        let dash = Dash {
            video: Vec::new(),
            audio: vec![audio(65_536), audio(196_608)],
        };
        assert_eq!(
            dash.best_audio().map(|audio| audio.bandwidth),
            Some(196_608)
        );
    }
}
