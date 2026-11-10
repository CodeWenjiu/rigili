use serde::Deserialize;

use super::{clean_text, deserialize_count, normalize_pic};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaSearch {
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub num_pages: u32,
    #[serde(default)]
    pub num_results: u64,
    #[serde(default, rename = "result")]
    pub items: Vec<SearchMedia>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSearchMedia")]
pub struct SearchMedia {
    pub season_id: u64,
    pub title: String,
    pub cover: String,
    pub index_show: String,
    pub subtitle: String,
    pub score: String,
    pub badge: Option<String>,
}

#[derive(Deserialize)]
struct RawSearchMedia {
    #[serde(default, deserialize_with = "deserialize_count")]
    season_id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    cover: String,
    #[serde(default)]
    index_show: String,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    styles: Vec<String>,
    #[serde(default)]
    media_score: Option<RawMediaScore>,
    #[serde(default)]
    desc: String,
    #[serde(default, deserialize_with = "deserialize_badges")]
    badges: Vec<RawBadge>,
}

#[derive(Deserialize)]
struct RawMediaScore {
    #[serde(default)]
    score: f64,
}

#[derive(Deserialize)]
struct RawBadge {
    #[serde(default)]
    text: String,
}

impl From<RawSearchMedia> for SearchMedia {
    fn from(raw: RawSearchMedia) -> Self {
        let styles = raw
            .styles
            .into_iter()
            .map(|style| clean_text(&style))
            .filter(|style| !style.is_empty())
            .collect::<Vec<_>>();
        let subtitle = if styles.is_empty() {
            clean_text(&raw.desc)
        } else {
            styles.join(" · ")
        };
        let score = raw
            .media_score
            .map(|score| score.score)
            .filter(|score| *score > 0.)
            .map(|score| format!("{score:.1}"))
            .unwrap_or_default();
        let badge = raw
            .badges
            .into_iter()
            .map(|badge| clean_text(&badge.text))
            .find(|text| !text.is_empty());

        Self {
            season_id: raw.season_id,
            title: clean_text(&raw.title),
            cover: normalize_pic(&raw.cover),
            index_show: clean_text(&raw.index_show),
            subtitle,
            score,
            badge,
        }
    }
}

fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum List {
        Sequence(Vec<String>),
        Text(String),
    }

    Ok(match Option::<List>::deserialize(deserializer)? {
        Some(List::Sequence(items)) => items,
        Some(List::Text(text)) => text
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect(),
        None => Vec::new(),
    })
}

fn deserialize_badges<'de, D>(deserializer: D) -> Result<Vec<RawBadge>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(
        match Option::<serde_json::Value>::deserialize(deserializer)? {
            Some(serde_json::Value::Array(items)) => items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect(),
            _ => Vec::new(),
        },
    )
}
