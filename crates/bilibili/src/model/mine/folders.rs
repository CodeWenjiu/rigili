use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FavFolders {
    #[serde(default)]
    pub list: Option<Vec<FavFolder>>,
}

impl FavFolders {
    pub fn into_items(self) -> Vec<FavFolder> {
        self.list.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct FavFolder {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub media_count: u64,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub ctime: i64,
    #[serde(default)]
    pub attr: u64,
}

impl FavFolder {
    pub fn is_private(&self) -> bool {
        self.attr & 1 != 0
    }

    pub fn is_default(&self) -> bool {
        self.attr == 0
    }
}

#[cfg(test)]
mod tests {
    use super::FavFolders;

    #[test]
    fn parses_folders_and_tolerates_a_null_list() {
        let folders: FavFolders = serde_json::from_str(
            r#"{"count":2,"has_more":false,"list":[
                {"id":1,"title":"默认收藏夹","media_count":42,"attr":0},
                {"id":2,"title":"私密收藏夹","media_count":7,"attr":23,"cover":"https://example.com/cover.jpg","ctime":1762646400}
            ]}"#,
        )
        .expect("folders");
        let folders = folders.into_items();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].id, 1);
        assert_eq!(folders[0].title, "默认收藏夹");
        assert_eq!(folders[0].media_count, 42);
        assert!(folders[0].is_default());
        assert!(!folders[0].is_private());
        assert!(!folders[1].is_default());
        assert!(folders[1].is_private());
        assert_eq!(folders[1].cover, "https://example.com/cover.jpg");
        assert_eq!(folders[1].ctime, 1_762_646_400);

        let empty: FavFolders =
            serde_json::from_str(r#"{"count":0,"list":null}"#).expect("empty folders");
        assert!(empty.into_items().is_empty());
    }
}
