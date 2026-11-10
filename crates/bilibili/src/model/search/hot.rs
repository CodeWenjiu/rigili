use serde::Deserialize;

use super::clean_text;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(from = "RawHotWords")]
pub struct HotWords {
    pub items: Vec<HotWord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawHotWord")]
pub struct HotWord {
    pub keyword: String,
    pub name: String,
    pub badge: Option<HotBadge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotBadge {
    New,
    Hot,
}

#[derive(Deserialize)]
struct RawHotWords {
    #[serde(default)]
    list: Vec<RawHotWord>,
}

#[derive(Deserialize)]
struct RawHotWord {
    #[serde(default)]
    keyword: String,
    #[serde(default)]
    show_name: String,
    #[serde(default)]
    word_type: u32,
}

impl From<RawHotWords> for HotWords {
    fn from(raw: RawHotWords) -> Self {
        Self {
            items: raw.list.into_iter().map(HotWord::from).collect(),
        }
    }
}

impl From<RawHotWord> for HotWord {
    fn from(raw: RawHotWord) -> Self {
        let show_name = raw.show_name.trim();
        let name = if show_name.is_empty() {
            clean_text(raw.keyword.trim())
        } else {
            clean_text(show_name)
        };
        let badge = match raw.word_type {
            4 => Some(HotBadge::New),
            5 => Some(HotBadge::Hot),
            _ => None,
        };

        Self {
            keyword: clean_text(raw.keyword.trim()),
            name,
            badge,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HotBadge, HotWords};

    #[test]
    fn maps_hot_word_badges_and_names() {
        let hot: HotWords = serde_json::from_str(
            r#"{
                "list": [
                    {"keyword": "新话题", "show_name": "新话题", "word_type": 4},
                    {"keyword": "热话题", "show_name": "热话题 ", "word_type": 5},
                    {"keyword": "普通话题", "word_type": 8}
                ]
            }"#,
        )
        .expect("hot words");

        assert_eq!(hot.items.len(), 3);
        assert_eq!(hot.items[0].badge, Some(HotBadge::New));
        assert_eq!(hot.items[1].badge, Some(HotBadge::Hot));
        assert_eq!(hot.items[1].name, "热话题");
        assert_eq!(hot.items[2].badge, None);
        assert_eq!(hot.items[2].name, "普通话题");
    }
}
