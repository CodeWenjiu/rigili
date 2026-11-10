use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcIndex {
    #[serde(default)]
    pub has_next: u32,
    #[serde(default)]
    pub list: Vec<PgcItem>,
    #[serde(default)]
    pub total: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcItem {
    #[serde(default)]
    pub season_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub index_show: String,
    #[serde(default, rename = "subTitle")]
    pub subtitle: String,
    #[serde(default)]
    pub score: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub season_type: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcSeason {
    #[serde(default)]
    pub season_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub episodes: Vec<PgcEpisode>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcEpisode {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    #[serde(default)]
    pub ep_id: u64,
    #[serde(default)]
    pub long_title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub duration: u64,
}
