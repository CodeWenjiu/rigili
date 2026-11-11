use crate::{ApiError, Client, Dynamics, SpaceCollections, SpaceVideos};

const DYNAMICS_URL: &str = "https://api.bilibili.com/x/polymer/web-dynamic/v1/feed/space";
const VIDEOS_URL: &str = "https://api.bilibili.com/x/space/wbi/arc/search";
const COLLECTIONS_URL: &str = "https://api.bilibili.com/x/polymer/web-space/seasons_series_list";
const VIDEO_PAGE_SIZE: u32 = 30;
const COLLECTION_PAGE_SIZE: u32 = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpaceVideoOrder {
    #[default]
    Latest,
    Plays,
}

impl SpaceVideoOrder {
    fn query(self) -> &'static str {
        match self {
            Self::Latest => "pubdate",
            Self::Plays => "click",
        }
    }
}

pub async fn dynamics(client: &Client, host_mid: u64, offset: &str) -> Result<Dynamics, ApiError> {
    client
        .get_wbi_json(
            DYNAMICS_URL,
            &[
                ("host_mid", host_mid.to_string()),
                ("offset", offset.to_owned()),
                (
                    "features",
                    "itemOpusStyle,listOnlyfans,opusBigCover,onlyfansVote".into(),
                ),
                ("web_location", "333.1387".into()),
            ],
        )
        .await
}

pub async fn videos(
    client: &Client,
    mid: u64,
    page: u32,
    order: SpaceVideoOrder,
    keyword: &str,
) -> Result<SpaceVideos, ApiError> {
    client
        .get_wbi_json(
            VIDEOS_URL,
            &[
                ("mid", mid.to_string()),
                ("ps", VIDEO_PAGE_SIZE.to_string()),
                ("pn", page.max(1).to_string()),
                ("tid", "0".into()),
                ("keyword", keyword.to_owned()),
                ("order", order.query().into()),
                ("platform", "web".into()),
                ("web_location", "1550101".into()),
                ("order_avoided", "true".into()),
            ],
        )
        .await
}

pub async fn collections(
    client: &Client,
    mid: u64,
    page: u32,
) -> Result<SpaceCollections, ApiError> {
    client
        .get_json(
            COLLECTIONS_URL,
            &[
                ("mid", mid.to_string()),
                ("page_num", page.max(1).to_string()),
                ("page_size", COLLECTION_PAGE_SIZE.to_string()),
            ],
        )
        .await
}
