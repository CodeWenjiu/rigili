use serde::Deserialize;

use super::video::{RawSearchVideo, SearchVideo};
use super::{clean_text, deserialize_count, normalize_pic};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserSearch {
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub num_pages: u32,
    #[serde(default)]
    pub num_results: u64,
    #[serde(default, rename = "result")]
    pub users: Vec<SearchUser>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSearchUser")]
pub struct SearchUser {
    pub mid: u64,
    pub name: String,
    pub face: String,
    pub fans: u64,
    pub video_count: u32,
    pub level: u32,
    pub sign: String,
    pub official: Option<String>,
    pub preview: Vec<SearchVideo>,
}

#[derive(Deserialize)]
struct RawSearchUser {
    #[serde(default)]
    mid: u64,
    #[serde(default)]
    uname: String,
    #[serde(default)]
    upic: String,
    #[serde(default, deserialize_with = "deserialize_count")]
    fans: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    videos: u64,
    #[serde(default)]
    level: u32,
    #[serde(default)]
    usign: String,
    #[serde(default)]
    official_verify: Option<RawOfficialVerify>,
    #[serde(default)]
    res: Vec<RawSearchVideo>,
}

#[derive(Deserialize)]
struct RawOfficialVerify {
    #[serde(default)]
    desc: String,
}

impl From<RawSearchUser> for SearchUser {
    fn from(raw: RawSearchUser) -> Self {
        Self {
            mid: raw.mid,
            name: clean_text(&raw.uname),
            face: normalize_pic(&raw.upic),
            fans: raw.fans,
            video_count: raw.videos.min(u32::MAX as u64) as u32,
            level: raw.level,
            sign: clean_text(&raw.usign),
            official: raw
                .official_verify
                .map(|verify| clean_text(&verify.desc))
                .filter(|desc| !desc.is_empty()),
            preview: raw.res.into_iter().map(SearchVideo::from).collect(),
        }
    }
}
