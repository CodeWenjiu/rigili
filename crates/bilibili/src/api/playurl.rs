use crate::{ApiError, Client, PlayUrl};

const PLAYURL_URL: &str = "https://api.bilibili.com/x/player/playurl";
const DASH_FEATURES: &str = "4048";

pub async fn fetch(client: &Client, bvid: &str, cid: u64) -> Result<PlayUrl, ApiError> {
    client
        .get_json(
            PLAYURL_URL,
            &[
                ("bvid", bvid.to_owned()),
                ("cid", cid.to_string()),
                ("fnval", DASH_FEATURES.to_owned()),
                ("fnver", "0".to_owned()),
                ("fourk", "1".to_owned()),
            ],
        )
        .await
}
