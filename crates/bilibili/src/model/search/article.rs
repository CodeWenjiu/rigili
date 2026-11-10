use serde::Deserialize;

use super::{clean_text, deserialize_count, normalize_pic};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArticleSearch {
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub num_pages: u32,
    #[serde(default)]
    pub num_results: u64,
    #[serde(default, rename = "result")]
    pub articles: Vec<SearchArticle>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSearchArticle")]
pub struct SearchArticle {
    pub id: u64,
    pub title: String,
    pub summary: String,
    pub cover: String,
    pub author: String,
    pub views: u64,
    pub likes: u64,
    pub replies: u64,
    pub pubdate: i64,
}

#[derive(Deserialize)]
struct RawSearchArticle {
    #[serde(default, deserialize_with = "deserialize_count")]
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    desc: String,
    #[serde(default, deserialize_with = "deserialize_image_urls")]
    image_urls: String,
    #[serde(default)]
    author: String,
    #[serde(default, deserialize_with = "deserialize_count")]
    view: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    like: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    reply: u64,
    #[serde(default)]
    pubdate: i64,
}

impl From<RawSearchArticle> for SearchArticle {
    fn from(raw: RawSearchArticle) -> Self {
        Self {
            id: raw.id,
            title: clean_text(&raw.title),
            summary: clean_text(&raw.desc),
            cover: raw
                .image_urls
                .split(',')
                .map(str::trim)
                .find(|url| !url.is_empty())
                .map(normalize_pic)
                .unwrap_or_default(),
            author: raw.author,
            views: raw.view,
            likes: raw.like,
            replies: raw.reply,
            pubdate: raw.pubdate,
        }
    }
}

fn deserialize_image_urls<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Urls {
        Text(String),
        Sequence(Vec<String>),
    }

    Ok(match Option::<Urls>::deserialize(deserializer)? {
        Some(Urls::Text(text)) => text,
        Some(Urls::Sequence(urls)) => urls.join(","),
        None => String::new(),
    })
}
