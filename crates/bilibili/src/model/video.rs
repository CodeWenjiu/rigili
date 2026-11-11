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

    pub fn preview_video(&self) -> Option<&DashVideo> {
        self.usable_videos()
            .min_by_key(|video| (video.height, video.codecid != 7, video.bandwidth))
    }

    fn usable_videos(&self) -> impl Iterator<Item = &DashVideo> {
        self.video
            .iter()
            .filter(|video| !video.base_url.is_empty() && video.width > 0 && video.height > 0)
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
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Recommend {
    #[serde(default)]
    pub item: Vec<RecommendVideo>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RecommendVideo {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    #[serde(default)]
    pub pic: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub pubdate: i64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub owner: Owner,
    #[serde(default, deserialize_with = "null_or_default")]
    pub stat: Stat,
}
fn null_or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
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
    fn selects_best_video_and_audio_tracks() {
        let dash = Dash {
            video: vec![video(13, 900_000), video(7, 600_000)],
            audio: vec![audio(65_536), audio(196_608)],
        };
        assert_eq!(dash.best_video().map(|video| video.codecid), Some(7));
        assert_eq!(
            dash.best_audio().map(|audio| audio.bandwidth),
            Some(196_608)
        );

        let no_avc = Dash {
            video: vec![video(12, 600_000), video(13, 900_000)],
            audio: Vec::new(),
        };
        assert_eq!(no_avc.best_video().map(|video| video.codecid), Some(13));
    }

    #[test]
    fn selects_the_smallest_preview_track() {
        let dash = Dash {
            video: vec![
                track(80, 1080, 7, 2_000_000),
                track(32, 480, 7, 600_000),
                track(16, 360, 12, 400_000),
                track(16, 360, 7, 500_000),
            ],
            audio: Vec::new(),
        };
        assert_eq!(
            dash.preview_video().map(|video| video.codecid),
            Some(7),
            "prefer the AVC track at the smallest size"
        );
        assert_eq!(dash.preview_video().map(|video| video.height), Some(360));

        let no_avc = Dash {
            video: vec![track(80, 1080, 13, 2_000_000), track(32, 480, 13, 600_000)],
            audio: Vec::new(),
        };
        assert_eq!(no_avc.preview_video().map(|video| video.id), Some(32));

        let unavailable = Dash {
            video: vec![DashVideo {
                base_url: String::new(),
                ..track(16, 360, 7, 500_000)
            }],
            audio: Vec::new(),
        };
        assert!(unavailable.preview_video().is_none());
    }

    fn track(id: u32, height: u32, codecid: u32, bandwidth: u64) -> DashVideo {
        DashVideo {
            id,
            base_url: format!("https://example.invalid/{id}-{codecid}.m4s"),
            bandwidth,
            width: height * 16 / 9,
            height,
            codecs: String::new(),
            codecid,
        }
    }
}
