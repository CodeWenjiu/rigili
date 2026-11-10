rigili_macro::mod_flat!(all, article, hot, live, media, suggest, user, video);

use serde::Deserialize;

fn clean_text(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            character if !in_tag => plain.push(character),
            _ => {}
        }
    }

    plain
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

fn normalize_pic(url: &str) -> String {
    match url.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

fn deserialize_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Count {
        Number(u64),
        Text(String),
    }

    Ok(match Option::<Count>::deserialize(deserializer)? {
        Some(Count::Number(value)) => value,
        Some(Count::Text(text)) => text.trim().parse().unwrap_or_default(),
        None => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::{SearchArticle, SearchMedia, SearchVideo};

    #[test]
    fn cleans_search_result_fields() {
        let video: SearchVideo = serde_json::from_str(
            r#"{
                "bvid": "BV1xx411c7mD",
                "title": "【<em class=\"keyword\">测试</em>】A&amp;B",
                "pic": "//i0.hdslb.com/bfs/archive/example.jpg",
                "author": "UP主",
                "duration": "1:02:03",
                "pubdate": 1762646400,
                "play": 12345,
                "like": 678,
                "video_review": 678
            }"#,
        )
        .expect("search video");

        assert_eq!(video.title, "【测试】A&B");
        assert_eq!(video.pic, "https://i0.hdslb.com/bfs/archive/example.jpg");
        assert_eq!(video.duration, 3_723);
        assert_eq!(video.danmaku, 678);
        assert_eq!(video.like, 678);
    }

    #[test]
    fn tolerates_partial_and_untyped_payloads() {
        let video: SearchVideo = serde_json::from_str(r#"{"bvid":"BV1"}"#).expect("search video");
        assert_eq!(video.duration, 0);
        assert_eq!(video.play, 0);
        assert_eq!(video.danmaku, 0);
        assert_eq!(video.like, 0);
        assert_eq!(video.pic, "");

        let media: SearchMedia = serde_json::from_str(
            r#"{
                "season_id": 1,
                "title": "作品",
                "styles": "",
                "badges": "",
                "media_score": null
            }"#,
        )
        .expect("search media");
        assert_eq!(media.season_id, 1);
        assert_eq!(media.title, "作品");
        assert!(media.subtitle.is_empty());
        assert!(media.score.is_empty());
        assert_eq!(media.badge, None);

        let article: SearchArticle = serde_json::from_str(
            r#"{
                "id": 1,
                "title": "标题",
                "image_urls": ["//i0.hdslb.com/a.jpg", "//i0.hdslb.com/b.jpg"]
            }"#,
        )
        .expect("search article");
        assert_eq!(article.cover, "https://i0.hdslb.com/a.jpg");
    }
}
