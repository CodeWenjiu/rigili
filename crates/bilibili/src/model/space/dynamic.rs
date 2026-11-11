use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Dynamics {
    #[serde(default)]
    pub items: Vec<DynamicItem>,
    #[serde(default)]
    pub offset: String,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicItem {
    #[serde(default)]
    pub id_str: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub modules: DynamicModules,
    #[serde(default)]
    pub orig: Option<Box<DynamicItem>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicModules {
    #[serde(default)]
    pub module_author: DynamicAuthor,
    #[serde(default)]
    pub module_dynamic: DynamicContent,
    #[serde(default)]
    pub module_stat: Option<DynamicStat>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicAuthor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub face: String,
    #[serde(default)]
    pub pub_time: String,
    #[serde(default)]
    pub pub_ts: String,
    #[serde(default)]
    pub pub_action: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicContent {
    #[serde(default)]
    pub desc: Option<DynamicDesc>,
    #[serde(default)]
    pub major: Option<DynamicMajor>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicDesc {
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicMajor {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub archive: Option<DynamicArchive>,
    #[serde(default)]
    pub draw: Option<DynamicDraw>,
    #[serde(default)]
    pub opus: Option<DynamicOpus>,
    #[serde(default)]
    pub none: Option<DynamicNone>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicArchive {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub duration_text: String,
    #[serde(default)]
    pub stat: DynamicArchiveStat,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicArchiveStat {
    #[serde(default)]
    pub play: String,
    #[serde(default)]
    pub danmaku: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicDraw {
    #[serde(default)]
    pub items: Vec<DynamicDrawItem>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicDrawItem {
    #[serde(default)]
    pub src: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicOpus {
    #[serde(default)]
    pub pics: Vec<DynamicOpusPic>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: Option<DynamicDesc>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicOpusPic {
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicNone {
    #[serde(default)]
    pub tips: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicStat {
    #[serde(default)]
    pub comment: DynamicCount,
    #[serde(default)]
    pub forward: DynamicCount,
    #[serde(default)]
    pub like: DynamicCount,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DynamicCount {
    #[serde(default)]
    pub count: u64,
}

impl DynamicItem {
    pub fn author(&self) -> &DynamicAuthor {
        &self.modules.module_author
    }

    pub fn text(&self) -> &str {
        self.modules
            .module_dynamic
            .desc
            .as_ref()
            .map(|desc| desc.text.as_str())
            .or_else(|| {
                self.modules
                    .module_dynamic
                    .major
                    .as_ref()
                    .and_then(|major| major.opus.as_ref())
                    .and_then(|opus| opus.summary.as_ref())
                    .map(|summary| summary.text.as_str())
            })
            .unwrap_or_default()
    }

    pub fn images(&self) -> Vec<&str> {
        let Some(major) = self.modules.module_dynamic.major.as_ref() else {
            return Vec::new();
        };
        if let Some(draw) = major.draw.as_ref() {
            return draw
                .items
                .iter()
                .map(|item| item.src.as_str())
                .filter(|src| !src.is_empty())
                .collect();
        }
        major
            .opus
            .as_ref()
            .map(|opus| {
                opus.pics
                    .iter()
                    .map(|pic| pic.url.as_str())
                    .filter(|url| !url.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn archive(&self) -> Option<&DynamicArchive> {
        self.modules
            .module_dynamic
            .major
            .as_ref()
            .and_then(|major| major.archive.as_ref())
    }

    pub fn forward(&self) -> Option<&DynamicItem> {
        self.orig.as_deref()
    }

    pub fn forward_tip(&self) -> Option<&str> {
        if let Some(orig) = self.orig.as_deref() {
            let tip = orig
                .modules
                .module_dynamic
                .major
                .as_ref()
                .and_then(|major| major.none.as_ref())
                .map(|none| none.tips.as_str())
                .filter(|tip| !tip.is_empty());
            if tip.is_some() {
                return tip;
            }
            return None;
        }
        (self.kind == "DYNAMIC_TYPE_FORWARD").then_some("源动态已被作者删除")
    }

    pub fn stat(&self) -> Option<&DynamicStat> {
        self.modules.module_stat.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::{DynamicItem, Dynamics};

    #[test]
    fn parses_forwarded_dynamic_with_deleted_source() {
        let page: Dynamics = serde_json::from_str(
            r#"{
                "items": [{
                    "id_str": "1",
                    "type": "DYNAMIC_TYPE_FORWARD",
                    "modules": {
                        "module_author": {"name": "UP", "face": "face.jpg", "pub_time": "6月7日", "pub_ts": "1780837029"},
                        "module_dynamic": {"desc": {"text": "转发理由"}},
                        "module_stat": {"comment": {"count": 1}, "forward": {"count": 2}, "like": {"count": 10}}
                    },
                    "orig": {
                        "id_str": "0",
                        "type": "DYNAMIC_TYPE_NONE",
                        "modules": {
                            "module_author": {"name": ""},
                            "module_dynamic": {"major": {"type": "MAJOR_TYPE_NONE", "none": {"tips": "源动态已被作者删除"}}}
                        }
                    }
                }],
                "offset": "42",
                "has_more": true
            }"#,
        )
        .expect("dynamics page");

        assert_eq!(page.offset, "42");
        assert!(page.has_more);

        let item = &page.items[0];
        assert_eq!(item.author().name, "UP");
        assert_eq!(item.text(), "转发理由");
        assert_eq!(item.forward_tip(), Some("源动态已被作者删除"));
        assert_eq!(item.stat().map(|stat| stat.like.count), Some(10));
    }

    #[test]
    fn parses_archive_and_draw_majors() {
        let item: DynamicItem = serde_json::from_str(
            r#"{
                "modules": {
                    "module_dynamic": {
                        "major": {
                            "type": "MAJOR_TYPE_ARCHIVE",
                            "archive": {
                                "bvid": "BV1xx411c7mD",
                                "title": "投稿视频",
                                "cover": "cover.jpg",
                                "duration_text": "00:19",
                                "stat": {"play": "59.8万", "danmaku": "52"}
                            }
                        }
                    }
                }
            }"#,
        )
        .expect("archive dynamic");

        let archive = item.archive().expect("archive");
        assert_eq!(archive.bvid, "BV1xx411c7mD");
        assert_eq!(archive.stat.play, "59.8万");
        assert!(item.images().is_empty());

        let draw: DynamicItem = serde_json::from_str(
            r#"{"modules":{"module_dynamic":{"major":{"type":"MAJOR_TYPE_DRAW","draw":{"items":[{"src":"a.jpg","width":100,"height":100}]}}}}}"#,
        )
        .expect("draw dynamic");
        assert_eq!(draw.images(), ["a.jpg"]);
    }
}
