use serde::Deserialize;

use crate::FavUpper;

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

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
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
    #[serde(default)]
    pub state: u64,
    #[serde(default)]
    pub upper: FavUpper,
}

impl FavFolder {
    pub fn is_private(&self) -> bool {
        self.attr & 1 != 0
    }

    pub fn is_default(&self) -> bool {
        self.attr == 0
    }

    pub fn is_valid(&self) -> bool {
        self.state == 0
    }

    pub fn owner(&self) -> &str {
        &self.upper.name
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CreatedFolder {
    pub id: u64,
}

#[cfg(test)]
mod tests {
    use super::FavFolders;

    #[test]
    fn parses_folders_and_tolerates_a_null_list() {
        let folders: FavFolders = serde_json::from_str(
            r#"{"count":2,"has_more":false,"list":[
                {"id":1,"title":"默认收藏夹","media_count":42,"attr":0,"state":0},
                {"id":2,"title":"私密收藏夹","media_count":7,"attr":23,"state":0,
                 "cover":"https://example.com/cover.jpg","ctime":1762646400,
                 "upper":{"mid":0,"name":"","face":""}}
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

    #[test]
    fn filters_invalid_collected_folders() {
        let folders: FavFolders = serde_json::from_str(
            r#"{"list":[
                {"id":1,"title":"该合集已失效","state":1,"attr":1},
                {"id":2,"title":"正常合集","state":0,"attr":22,
                 "upper":{"mid":7,"name":"订阅来源"}}
            ]}"#,
        )
        .expect("collected folders");
        let folders = folders.into_items();
        assert!(!folders[0].is_valid());
        assert!(folders[1].is_valid());
        assert_eq!(folders[1].owner(), "订阅来源");
    }
}
