use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceVideos {
    #[serde(default)]
    pub page: SpaceVideosPage,
    #[serde(default)]
    pub list: SpaceVideoList,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceVideosPage {
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub pn: u32,
    #[serde(default)]
    pub ps: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceVideoList {
    #[serde(default)]
    pub vlist: Vec<SpaceVideo>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceVideo {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pic: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub length: String,
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub video_review: u64,
}

#[cfg(test)]
mod tests {
    use super::SpaceVideos;

    #[test]
    fn parses_uploaded_videos_with_page() {
        let page: SpaceVideos = serde_json::from_str(
            r#"{
                "page": {"count": 11, "pn": 1, "ps": 30},
                "list": {
                    "vlist": [{
                        "bvid": "BV1HJ4m1V7F4",
                        "title": "投稿视频",
                        "pic": "cover.jpg",
                        "author": "UP",
                        "created": 1712426819,
                        "length": "01:03",
                        "play": 703,
                        "video_review": 11
                    }]
                }
            }"#,
        )
        .expect("uploaded videos");

        assert_eq!(page.page.count, 11);
        assert_eq!(page.page.pn, 1);
        assert_eq!(page.list.vlist.len(), 1);
        assert_eq!(page.list.vlist[0].bvid, "BV1HJ4m1V7F4");
        assert_eq!(page.list.vlist[0].play, 703);
    }
}
