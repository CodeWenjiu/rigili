use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceUser {
    #[serde(default)]
    pub mid: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub face: String,
    #[serde(default)]
    pub sign: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub top_photo: String,
    #[serde(default)]
    pub official: SpaceOfficial,
    #[serde(default)]
    pub vip: SpaceVip,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceOfficial {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub desc: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceVip {
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub label: SpaceVipLabel,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceVipLabel {
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceRelation {
    #[serde(default)]
    pub mid: u64,
    #[serde(default)]
    pub following: u64,
    #[serde(default)]
    pub follower: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceUpStat {
    #[serde(default)]
    pub archive: SpaceArchiveStat,
    #[serde(default)]
    pub likes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceArchiveStat {
    #[serde(default)]
    pub view: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SpaceProfile {
    pub user: SpaceUser,
    pub relation: Option<SpaceRelation>,
    pub up: Option<SpaceUpStat>,
}

impl SpaceProfile {
    pub fn following(&self) -> Option<u64> {
        self.relation.as_ref().map(|relation| relation.following)
    }

    pub fn follower(&self) -> Option<u64> {
        self.relation.as_ref().map(|relation| relation.follower)
    }

    pub fn likes(&self) -> Option<u64> {
        self.up.as_ref().map(|up| up.likes)
    }

    pub fn official_title(&self) -> Option<&str> {
        let title = self.user.official.title.as_str();
        (!title.is_empty()).then_some(title)
    }

    pub fn vip_label(&self) -> Option<&str> {
        let label = self.user.vip.label.text.as_str();
        (self.user.vip.status == 1 && !label.is_empty()).then_some(label)
    }
}

#[cfg(test)]
mod tests {
    use super::{SpaceRelation, SpaceUpStat, SpaceUser};

    #[test]
    fn parses_space_user_info_and_tolerates_missing_fields() {
        let user: SpaceUser = serde_json::from_str(
            r#"{
                "mid": 35161381,
                "name": "温九_speechless",
                "face": "https://example.com/face.jpg",
                "sign": "简介",
                "level": 6,
                "top_photo": "https://example.com/banner.jpg",
                "official": {"title": "bilibili 知名UP主", "desc": ""},
                "vip": {"status": 1, "label": {"text": "年度大会员"}}
            }"#,
        )
        .expect("space user fixture");

        assert_eq!(user.mid, 35_161_381);
        assert_eq!(user.level, 6);
        assert_eq!(user.top_photo, "https://example.com/banner.jpg");
        assert_eq!(user.official.title, "bilibili 知名UP主");
        assert_eq!(user.vip.label.text, "年度大会员");

        let user: SpaceUser = serde_json::from_str(r#"{"mid": 1, "name": "UP"}"#)
            .expect("minimal space user fixture");
        assert!(user.top_photo.is_empty());
        assert!(user.official.title.is_empty());
        assert_eq!(user.vip.status, 0);
    }

    #[test]
    fn parses_relation_and_up_stats() {
        let relation: SpaceRelation =
            serde_json::from_str(r#"{"mid": 1, "following": 42, "follower": 25000}"#)
                .expect("relation fixture");
        assert_eq!(relation.following, 42);
        assert_eq!(relation.follower, 25_000);

        let up: SpaceUpStat = serde_json::from_str(
            r#"{"archive": {"view": 57381, "danmaku": 10}, "article": {"view": 0}, "likes": 59000}"#,
        )
        .expect("upstat fixture");
        assert_eq!(up.archive.view, 57_381);
        assert_eq!(up.likes, 59_000);
    }
}
