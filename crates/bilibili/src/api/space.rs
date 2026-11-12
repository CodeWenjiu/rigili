use std::collections::BTreeMap;

use crate::{
    ApiError, Client, Dynamics, SpaceCollections, SpaceProfile, SpaceRelation, SpaceRelationStatus,
    SpaceUpStat, SpaceUser, SpaceVideos,
};

const INFO_URL: &str = "https://api.bilibili.com/x/space/wbi/acc/info";
const DYNAMICS_URL: &str = "https://api.bilibili.com/x/polymer/web-dynamic/v1/feed/space";
const VIDEOS_URL: &str = "https://api.bilibili.com/x/space/wbi/arc/search";
const RELATION_URL: &str = "https://api.bilibili.com/x/relation/stat";
const RELATION_STATUS_URL: &str = "https://api.bilibili.com/x/relation";
const RELATION_STATUSES_URL: &str = "https://api.bilibili.com/x/relation/relations";
const RELATION_MODIFY_URL: &str = "https://api.bilibili.com/x/relation/modify";
const UPSTAT_URL: &str = "https://api.bilibili.com/x/space/upstat";
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

pub async fn user_info(client: &Client, mid: u64) -> Result<SpaceUser, ApiError> {
    client
        .get_wbi_json(
            INFO_URL,
            &[
                ("mid", mid.to_string()),
                ("platform", "web".into()),
                ("web_location", "1550101".into()),
            ],
        )
        .await
}

pub async fn relation_stat(client: &Client, mid: u64) -> Result<SpaceRelation, ApiError> {
    client
        .get_json(RELATION_URL, &[("vmid", mid.to_string())])
        .await
}

pub async fn relation_status(client: &Client, mid: u64) -> Result<SpaceRelationStatus, ApiError> {
    client
        .get_json(RELATION_STATUS_URL, &[("fid", mid.to_string())])
        .await
}

pub async fn relation_statuses(
    client: &Client,
    mids: &[u64],
) -> Result<BTreeMap<u64, SpaceRelationStatus>, ApiError> {
    if mids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let fids = mids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let statuses: BTreeMap<String, SpaceRelationStatus> = client
        .get_json_or_default(RELATION_STATUSES_URL, &[("fids", fids)])
        .await?;
    Ok(statuses
        .into_iter()
        .filter_map(|(mid, status)| mid.parse().ok().map(|mid| (mid, status)))
        .collect())
}

pub async fn modify_relation(client: &Client, mid: u64, follow: bool) -> Result<(), ApiError> {
    client
        .post_form_ack(
            RELATION_MODIFY_URL,
            &[
                ("fid", mid.to_string()),
                ("act", if follow { "1" } else { "2" }.to_owned()),
                ("re_src", "11".to_owned()),
                ("csrf", client.csrf()?),
            ],
        )
        .await
}

pub async fn up_stat(client: &Client, mid: u64) -> Result<SpaceUpStat, ApiError> {
    client
        .get_json(UPSTAT_URL, &[("mid", mid.to_string())])
        .await
}

pub async fn profile(client: &Client, mid: u64) -> Result<SpaceProfile, ApiError> {
    let user = user_info(client, mid).await?;
    let relation = relation_stat(client, mid).await.ok();
    let up = up_stat(client, mid).await.ok();
    let viewer = if client.cookie_values().contains_key("SESSDATA") {
        relation_status(client, mid).await.ok()
    } else {
        None
    };
    Ok(SpaceProfile {
        user,
        relation,
        up,
        viewer,
    })
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
    fetch_videos(client, mid, page, VIDEO_PAGE_SIZE, order, keyword).await
}

pub async fn preview_videos(client: &Client, mid: u64) -> Result<SpaceVideos, ApiError> {
    fetch_videos(client, mid, 1, VIDEO_PAGE_SIZE, SpaceVideoOrder::Latest, "").await
}

async fn fetch_videos(
    client: &Client,
    mid: u64,
    page: u32,
    page_size: u32,
    order: SpaceVideoOrder,
    keyword: &str,
) -> Result<SpaceVideos, ApiError> {
    client
        .get_wbi_json(
            VIDEOS_URL,
            &[
                ("mid", mid.to_string()),
                ("ps", page_size.to_string()),
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
