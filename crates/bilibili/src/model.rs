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
