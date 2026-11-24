use crate::{
    ApiError, Client, EpisodeId, PlayerInfo, SubtitleFile, VideoDetail, VideoInfo, VideoShot,
};

const VIEW_URL: &str = "https://api.bilibili.com/x/web-interface/view";
const DETAIL_URL: &str = "https://api.bilibili.com/x/web-interface/view/detail";
const VIDEOSHOT_URL: &str = "https://api.bilibili.com/x/player/videoshot";
const PLAYER_URL: &str = "https://api.bilibili.com/x/player/wbi/v2";

impl Client {
    pub async fn view(&self, bvid: &str) -> Result<VideoInfo, ApiError> {
        self.get_json(VIEW_URL, &[("bvid", bvid.to_owned())]).await
    }

    pub async fn video_detail(&self, bvid: &str) -> Result<VideoDetail, ApiError> {
        self.get_json(DETAIL_URL, &[("bvid", bvid.to_owned())])
            .await
    }

    pub async fn videoshot(&self, bvid: &str, cid: u64) -> Result<VideoShot, ApiError> {
        self.get_json(
            VIDEOSHOT_URL,
            &[
                ("bvid", bvid.to_owned()),
                ("cid", cid.to_string()),
                ("index", "1".to_owned()),
            ],
        )
        .await
    }

    pub async fn player_info(
        &self,
        bvid: &str,
        cid: u64,
        episode: Option<EpisodeId>,
    ) -> Result<PlayerInfo, ApiError> {
        let mut params = vec![("bvid", bvid.to_owned()), ("cid", cid.to_string())];
        if let Some(episode) = episode {
            params.push(("ep_id", episode.ep_id.to_string()));
            params.push(("season_id", episode.season_id.to_string()));
        }
        self.get_wbi_json(PLAYER_URL, &params).await
    }

    pub async fn subtitle(&self, url: &str) -> Result<SubtitleFile, ApiError> {
        self.get_raw(url).await
    }
}
