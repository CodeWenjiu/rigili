use crate::{ApiError, Chapter, Client, PlayerInfo, VideoDetail, VideoInfo, VideoShot};

const VIEW_URL: &str = "https://api.bilibili.com/x/web-interface/view";
const DETAIL_URL: &str = "https://api.bilibili.com/x/web-interface/view/detail";
const VIDEOSHOT_URL: &str = "https://api.bilibili.com/x/player/videoshot";
const PLAYER_URL: &str = "https://api.bilibili.com/x/player/wbi/v2";

pub async fn fetch(client: &Client, bvid: &str) -> Result<VideoInfo, ApiError> {
    client
        .get_json(VIEW_URL, &[("bvid", bvid.to_owned())])
        .await
}

pub async fn detail(client: &Client, bvid: &str) -> Result<VideoDetail, ApiError> {
    client
        .get_json(DETAIL_URL, &[("bvid", bvid.to_owned())])
        .await
}

pub async fn videoshot(client: &Client, bvid: &str, cid: u64) -> Result<VideoShot, ApiError> {
    client
        .get_json(
            VIDEOSHOT_URL,
            &[
                ("bvid", bvid.to_owned()),
                ("cid", cid.to_string()),
                ("index", "1".to_owned()),
            ],
        )
        .await
}

pub async fn chapters(client: &Client, bvid: &str, cid: u64) -> Result<Vec<Chapter>, ApiError> {
    let info: PlayerInfo = client
        .get_wbi_json(
            PLAYER_URL,
            &[("bvid", bvid.to_owned()), ("cid", cid.to_string())],
        )
        .await?;
    Ok(info.chapters())
}
