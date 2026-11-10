use serde::Deserialize;

use super::{clean_text, deserialize_count, normalize_pic};


#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawLiveSearch")]
pub struct LiveSearch {
    pub page: u32,
    pub num_pages: u32,
    pub num_results: u64,
    pub rooms: Vec<SearchLiveRoom>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSearchLiveRoom")]
pub struct SearchLiveRoom {
    pub room_id: u64,
    pub uid: u64,
    pub name: String,
    pub face: String,
    pub cover: String,
    pub title: String,
    pub area: String,
    pub online: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLiveSearch {
    #[serde(default)]
    page: u32,
    #[serde(default)]
    num_pages: u32,
    #[serde(default)]
    num_results: u64,
    #[serde(default)]
    result: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawSearchLiveRoom {
    #[serde(default, deserialize_with = "deserialize_count")]
    roomid: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    uid: u64,
    #[serde(default)]
    uname: String,
    #[serde(default)]
    uface: String,
    #[serde(default)]
    cover: String,
    #[serde(default)]
    user_cover: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    cate_name: String,
    #[serde(default, deserialize_with = "deserialize_count")]
    online: u64,
}

impl From<RawLiveSearch> for LiveSearch {
    fn from(raw: RawLiveSearch) -> Self {
        let rooms = raw
            .result
            .and_then(|result| result.get("live_room").cloned())
            .and_then(|rooms| serde_json::from_value::<Vec<RawSearchLiveRoom>>(rooms).ok())
            .unwrap_or_default()
            .into_iter()
            .map(SearchLiveRoom::from)
            .collect();

        Self {
            page: raw.page,
            num_pages: raw.num_pages,
            num_results: raw.num_results,
            rooms,
        }
    }
}

impl From<RawSearchLiveRoom> for SearchLiveRoom {
    fn from(raw: RawSearchLiveRoom) -> Self {
        let cover = if raw.cover.is_empty() {
            raw.user_cover
        } else {
            raw.cover
        };

        Self {
            room_id: raw.roomid,
            uid: raw.uid,
            name: clean_text(&raw.uname),
            face: normalize_pic(&raw.uface),
            cover: normalize_pic(&cover),
            title: clean_text(&raw.title),
            area: clean_text(&raw.cate_name),
            online: raw.online,
        }
    }
}
