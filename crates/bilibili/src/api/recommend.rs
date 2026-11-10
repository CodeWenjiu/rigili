use crate::{ApiError, Client, Recommend};

const RECOMMEND_URL: &str = "https://api.bilibili.com/x/web-interface/wbi/index/top/feed/rcmd";

pub async fn fetch(client: &Client, fresh_idx: u32, page_size: u32) -> Result<Recommend, ApiError> {
    let fresh_idx = fresh_idx.max(1);
    let page_size = page_size.clamp(1, 30);

    client
        .get_wbi_json(
            RECOMMEND_URL,
            &[
                ("web_location", "1430654".into()),
                ("y_num", "4".into()),
                ("fresh_type", "4".into()),
                ("feed_version", "V8".into()),
                ("fresh_idx", fresh_idx.to_string()),
                ("fresh_idx_1h", fresh_idx.to_string()),
                ("brush", "0".into()),
                ("homepage_ver", "1".into()),
                ("ps", page_size.to_string()),
            ],
        )
        .await
}

#[cfg(test)]
mod tests {
    use crate::Recommend;

    const RECOMMEND_FIXTURE: &str = include_str!("../../fixtures/recommend.json");

    #[derive(serde::Deserialize)]
    struct Envelope {
        data: Recommend,
    }

    #[test]
    fn parses_recommend_fixture() {
        let envelope: Envelope =
            serde_json::from_str(RECOMMEND_FIXTURE).expect("recommend fixture");
        let recommend = envelope.data;

        assert_eq!(recommend.item.len(), 2);
        assert_eq!(recommend.item[0].bvid, "BV1TgWp6MEHJ");
        assert_eq!(recommend.item[0].cid, 43_448_467_681);
        assert_eq!(recommend.item[0].owner.name, "测试UP");
        assert!(recommend.item[1].bvid.is_empty());
    }
}
