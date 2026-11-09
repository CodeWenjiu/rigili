use crate::{ApiError, Client, VideoInfo};

const VIEW_URL: &str = "https://api.bilibili.com/x/web-interface/view";

pub async fn fetch(client: &Client, bvid: &str) -> Result<VideoInfo, ApiError> {
    client
        .get_json(VIEW_URL, &[("bvid", bvid.to_owned())])
        .await
}
