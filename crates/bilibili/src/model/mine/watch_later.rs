use serde::Deserialize;

use crate::{Owner, Stat};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct WatchLater {
    #[serde(default)]
    pub list: Option<Vec<WatchLaterItem>>,
    #[serde(default)]
    pub count: u64,
}

impl WatchLater {
    pub fn into_items(self) -> Vec<WatchLaterItem> {
        self.list.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WatchLaterItem {
    #[serde(default)]
    pub aid: u64,
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
    #[serde(default)]
    pub owner: Owner,
    #[serde(default)]
    pub stat: Stat,
}

#[cfg(test)]
mod tests {
    use super::WatchLater;

    #[test]
    fn parses_watch_later_and_tolerates_a_null_list() {
        let later: WatchLater = serde_json::from_str(
            r#"{
                "count": 1,
                "list": [{
                    "aid": 99,
                    "bvid": "BV1xx411c7mD",
                    "cid": 123,
                    "pic": "https://example.com/cover.jpg",
                    "title": "稍后再看",
                    "duration": 60,
                    "pubdate": 1762646400,
                    "owner": {"mid": 1, "name": "UP 主"},
                    "stat": {"view": 100, "danmaku": 2}
                }]
            }"#,
        )
        .expect("watch later");

        let items = later.into_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].aid, 99);
        assert_eq!(items[0].cid, 123);
        assert_eq!(items[0].owner.name, "UP 主");
        assert_eq!(items[0].stat.view, 100);

        let empty: WatchLater =
            serde_json::from_str(r#"{"count":0,"list":null}"#).expect("empty watch later");
        assert!(empty.into_items().is_empty());
    }
}
