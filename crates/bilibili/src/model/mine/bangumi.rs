use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct BangumiFollows {
    #[serde(default)]
    pub list: Vec<BangumiFollow>,
    #[serde(default)]
    pub total: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct BangumiFollow {
    #[serde(default)]
    pub season_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub progress: String,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub is_finish: i64,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub season_type_name: String,
    #[serde(default)]
    pub new_ep: Option<BangumiNewEp>,
    #[serde(default)]
    pub rating: Option<BangumiRating>,
}

impl BangumiFollow {
    pub fn latest_ep(&self) -> &str {
        self.new_ep
            .as_ref()
            .map(|episode| episode.index_show.as_str())
            .unwrap_or("")
    }

    pub fn score(&self) -> Option<f64> {
        self.rating
            .as_ref()
            .map(|rating| rating.score)
            .filter(|score| *score > 0.)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct BangumiNewEp {
    #[serde(default)]
    pub index_show: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct BangumiRating {
    #[serde(default)]
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::BangumiFollows;

    #[test]
    fn parses_followed_bangumi_and_tolerates_missing_metadata() {
        let follows: BangumiFollows = serde_json::from_str(
            r#"{
                "total": 170,
                "list": [{
                    "season_id": 1733,
                    "title": "罗小黑战记",
                    "cover": "https://example.com/cover.jpg",
                    "progress": "看到第40话 5:16",
                    "total_count": 0,
                    "is_finish": 0,
                    "badge": "独家",
                    "season_type_name": "国创",
                    "new_ep": {"index_show": "更新至第42话"},
                    "rating": {"score": 9.9}
                }]
            }"#,
        )
        .expect("bangumi follows");

        assert_eq!(follows.total, 170);
        let item = &follows.list[0];
        assert_eq!(item.season_id, 1733);
        assert_eq!(item.latest_ep(), "更新至第42话");
        assert_eq!(item.score(), Some(9.9));

        let follows: BangumiFollows = serde_json::from_str(
            r#"{"list":[{"season_id":1,"title":"测试","new_ep":null,"rating":null}]}"#,
        )
        .expect("bangumi follows");
        assert_eq!(follows.list[0].latest_ep(), "");
        assert_eq!(follows.list[0].score(), None);
    }
}
