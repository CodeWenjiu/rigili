use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FavItems {
    #[serde(default)]
    pub medias: Option<Vec<FavItem>>,
    #[serde(default)]
    pub has_more: bool,
}

impl FavItems {
    pub fn into_items(self) -> Vec<FavItem> {
        self.medias.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct FavItem {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub pubtime: i64,
    #[serde(default)]
    pub upper: FavUpper,
    #[serde(default)]
    pub cnt_info: FavCounts,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FavUpper {
    #[serde(default)]
    pub mid: u64,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FavCounts {
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub danmaku: u64,
}

#[cfg(test)]
mod tests {
    use super::FavItems;

    #[test]
    fn parses_favorite_media_and_tolerates_an_empty_folder() {
        let items: FavItems = serde_json::from_str(
            r#"{
                "has_more": true,
                "medias": [{
                    "bvid": "BV1xx411c7mD",
                    "title": "收藏的视频",
                    "cover": "https://example.com/cover.jpg",
                    "duration": 125,
                    "pubtime": 1762646400,
                    "upper": {"mid": 42, "name": "UP 主"},
                    "cnt_info": {"play": 12345, "danmaku": 67}
                }]
            }"#,
        )
        .expect("favorite items");

        assert!(items.has_more);
        let items = items.into_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].bvid, "BV1xx411c7mD");
        assert_eq!(items[0].upper.mid, 42);
        assert_eq!(items[0].upper.name, "UP 主");
        assert_eq!(items[0].cnt_info.play, 12345);
        assert_eq!(items[0].cnt_info.danmaku, 67);

        let empty: FavItems =
            serde_json::from_str(r#"{"has_more":false,"medias":null}"#).expect("empty folder");
        assert!(empty.into_items().is_empty());
    }
}
