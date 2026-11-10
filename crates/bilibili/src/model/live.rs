use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct LiveRoom {
    #[serde(default)]
    pub roomid: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub online: u64,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub system_cover: String,
    #[serde(default)]
    pub area_name: String,
    #[serde(default)]
    pub face: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct LivePlayUrl {
    #[serde(default)]
    pub durl: Vec<LiveStream>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct LiveStream {
    #[serde(default)]
    pub url: String,
}
