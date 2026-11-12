use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct HistoryPage {
    #[serde(default)]
    pub cursor: HistoryCursor,
    #[serde(default)]
    pub list: Vec<HistoryItem>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct HistoryCursor {
    #[serde(default)]
    pub max: u64,
    #[serde(default)]
    pub view_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct HistoryItem {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_mid: u64,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub view_at: i64,
    #[serde(default)]
    pub history: HistoryRef,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct HistoryRef {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub business: String,
}

#[cfg(test)]
mod tests {
    use super::HistoryPage;

    #[test]
    fn parses_history_entries_with_cursor() {
        let page: HistoryPage = serde_json::from_str(
            r#"{
                "cursor": {"max": 42, "view_at": 1762646400, "business": "archive", "ps": 20},
                "list": [{
                    "title": "看过的视频",
                    "cover": "https://example.com/cover.jpg",
                    "author_name": "UP 主",
                    "author_mid": 42,
                    "duration": 90,
                    "view_at": 1762646400,
                    "history": {"bvid": "BV1xx411c7mD", "business": "archive"}
                }]
            }"#,
        )
        .expect("history page");

        assert_eq!(page.cursor.max, 42);
        assert_eq!(page.cursor.view_at, 1_762_646_400);
        assert_eq!(page.list.len(), 1);
        assert_eq!(page.list[0].history.bvid, "BV1xx411c7mD");
        assert_eq!(page.list[0].author_name, "UP 主");
        assert_eq!(page.list[0].author_mid, 42);
    }
}
