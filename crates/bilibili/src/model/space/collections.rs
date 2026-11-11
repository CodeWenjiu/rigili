use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceCollections {
    #[serde(default)]
    pub items_lists: CollectionLists,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CollectionLists {
    #[serde(default)]
    pub page: CollectionPage,
    #[serde(default)]
    pub seasons_list: Vec<SeasonEntry>,
    #[serde(default)]
    pub series_list: Vec<SeriesEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CollectionPage {
    #[serde(default)]
    pub page_num: u32,
    #[serde(default)]
    pub page_size: u32,
    #[serde(default)]
    pub total: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SeasonEntry {
    #[serde(default)]
    pub meta: SeasonMeta,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SeasonMeta {
    #[serde(default)]
    pub season_id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub ptime: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SeriesEntry {
    #[serde(default)]
    pub meta: SeriesMeta,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SeriesMeta {
    #[serde(default)]
    pub series_id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub last_update_ts: i64,
}

impl CollectionLists {
    pub fn is_empty(&self) -> bool {
        self.seasons_list.is_empty() && self.series_list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::SpaceCollections;

    #[test]
    fn parses_seasons_and_series() {
        let collections: SpaceCollections = serde_json::from_str(
            r#"{
                "items_lists": {
                    "page": {"page_num": 1, "page_size": 20, "total": 2},
                    "seasons_list": [{
                        "meta": {
                            "season_id": 123,
                            "name": "合集",
                            "cover": "season.jpg",
                            "description": "说明",
                            "total": 10,
                            "ptime": 1700000000
                        }
                    }],
                    "series_list": [{
                        "meta": {
                            "series_id": 456,
                            "name": "系列",
                            "cover": "series.jpg",
                            "description": "说明",
                            "total": 3,
                            "last_update_ts": 1700000000
                        }
                    }]
                }
            }"#,
        )
        .expect("collections");

        assert_eq!(collections.items_lists.page.total, 2);
        assert!(!collections.items_lists.is_empty());
        assert_eq!(collections.items_lists.seasons_list[0].meta.season_id, 123);
        assert_eq!(collections.items_lists.series_list[0].meta.series_id, 456);
    }
}
