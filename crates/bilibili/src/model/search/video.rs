use serde::Deserialize;

use super::{clean_text, deserialize_count, normalize_pic};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoSearch {
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub num_pages: u32,
    #[serde(default)]
    pub num_results: u64,
    #[serde(default, rename = "result")]
    pub videos: Vec<SearchVideo>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSearchVideo")]
pub struct SearchVideo {
    pub bvid: String,
    pub title: String,
    pub pic: String,
    pub author: String,
    pub duration: u64,
    pub pubdate: i64,
    pub play: u64,
    pub danmaku: u64,
    pub like: u64,
}

#[derive(Deserialize)]
pub(super) struct RawSearchVideo {
    #[serde(default)]
    bvid: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    pic: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    pubdate: i64,
    #[serde(default, deserialize_with = "deserialize_count")]
    play: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    like: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    video_review: u64,
    #[serde(
        default,
        deserialize_with = "deserialize_count",
        rename = "danmaku",
        alias = "dm"
    )]
    danmaku_count: u64,
}

impl From<RawSearchVideo> for SearchVideo {
    fn from(raw: RawSearchVideo) -> Self {
        Self {
            bvid: raw.bvid,
            title: clean_text(&raw.title),
            pic: normalize_pic(&raw.pic),
            author: raw.author,
            duration: parse_duration(&raw.duration),
            pubdate: raw.pubdate,
            play: raw.play,
            danmaku: raw.video_review.max(raw.danmaku_count),
            like: raw.like,
        }
    }
}

fn parse_duration(text: &str) -> u64 {
    let mut total = 0u64;
    for part in text.trim().split(':') {
        let Ok(value) = part.trim().parse::<u64>() else {
            return 0;
        };
        total = total.saturating_mul(60).saturating_add(value);
    }
    total
}
