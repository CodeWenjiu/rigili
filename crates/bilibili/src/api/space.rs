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

impl Client {
    pub async fn space_profile(&self, mid: u64) -> Result<SpaceProfile, ApiError> {
        let user = self.space_user_info(mid).await?;
        let relation = self.space_relation_stat(mid).await.ok();
        let up = self.space_up_stat(mid).await.ok();
        let viewer = if self.cookie_values().contains_key("SESSDATA") {
            self.relation_status(mid).await.ok()
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

    pub async fn modify_relation(&self, mid: u64, follow: bool) -> Result<(), ApiError> {
        self.post_form_ack(
            RELATION_MODIFY_URL,
            &[
                ("fid", mid.to_string()),
                ("act", if follow { "1" } else { "2" }.to_owned()),
                ("re_src", "11".to_owned()),
                ("csrf", self.csrf()?),
            ],
        )
        .await
    }

    pub async fn relation_status(&self, mid: u64) -> Result<SpaceRelationStatus, ApiError> {
        self.get_json(RELATION_STATUS_URL, &[("fid", mid.to_string())])
            .await
    }

    pub async fn relation_statuses(
        &self,
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
        let statuses: BTreeMap<String, SpaceRelationStatus> = self
            .get_json_or_default(RELATION_STATUSES_URL, &[("fids", fids)])
            .await?;
        Ok(statuses
            .into_iter()
            .filter_map(|(mid, status)| mid.parse().ok().map(|mid| (mid, status)))
            .collect())
    }

    pub async fn dynamics(&self, host_mid: u64, offset: &str) -> Result<Dynamics, ApiError> {
        self.get_wbi_json(
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

    pub async fn space_videos(
        &self,
        mid: u64,
        page: u32,
        order: SpaceVideoOrder,
        keyword: &str,
    ) -> Result<SpaceVideos, ApiError> {
        self.space_videos_page(mid, page, VIDEO_PAGE_SIZE, order, keyword)
            .await
    }

    pub async fn space_preview_videos(&self, mid: u64) -> Result<SpaceVideos, ApiError> {
        self.space_videos_page(mid, 1, VIDEO_PAGE_SIZE, SpaceVideoOrder::Latest, "")
            .await
    }

    pub async fn collections(&self, mid: u64, page: u32) -> Result<SpaceCollections, ApiError> {
        self.get_json(
            COLLECTIONS_URL,
            &[
                ("mid", mid.to_string()),
                ("page_num", page.max(1).to_string()),
                ("page_size", COLLECTION_PAGE_SIZE.to_string()),
            ],
        )
        .await
    }

    async fn space_user_info(&self, mid: u64) -> Result<SpaceUser, ApiError> {
        self.get_wbi_json(
            INFO_URL,
            &[
                ("mid", mid.to_string()),
                ("platform", "web".into()),
                ("web_location", "1550101".into()),
            ],
        )
        .await
    }

    async fn space_relation_stat(&self, mid: u64) -> Result<SpaceRelation, ApiError> {
        self.get_json(RELATION_URL, &[("vmid", mid.to_string())])
            .await
    }

    async fn space_up_stat(&self, mid: u64) -> Result<SpaceUpStat, ApiError> {
        self.get_json(UPSTAT_URL, &[("mid", mid.to_string())]).await
    }

    async fn space_videos_page(
        &self,
        mid: u64,
        page: u32,
        page_size: u32,
        order: SpaceVideoOrder,
        keyword: &str,
    ) -> Result<SpaceVideos, ApiError> {
        self.get_wbi_json(
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
}
