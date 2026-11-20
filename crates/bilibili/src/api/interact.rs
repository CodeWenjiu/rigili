use crate::http::unix_timestamp;
use crate::{ApiError, ArchiveRelation, Client};

const RELATION_URL: &str = "https://api.bilibili.com/x/web-interface/archive/relation";
const LIKE_URL: &str = "https://api.bilibili.com/x/web-interface/archive/like";
const COIN_URL: &str = "https://api.bilibili.com/x/web-interface/coin/add";
const FAVORITE_URL: &str = "https://api.bilibili.com/x/v3/fav/resource/deal";

// The video page posts these alongside the action itself; risk control rejects
// a bare csrf form on some accounts.
const WEB_SPMID: &str = "333.788.0.0";
const WEB_STATISTICS: &str = r#"{"appId":100,"platform":5}"#;

impl Client {
    pub async fn archive_relation(&self, aid: u64) -> Result<ArchiveRelation, ApiError> {
        self.get_json(RELATION_URL, &[("aid", aid.to_string())])
            .await
    }

    pub async fn set_like(&self, aid: u64, like: bool) -> Result<(), ApiError> {
        self.post_form_ack(
            LIKE_URL,
            &[
                ("aid", aid.to_string()),
                ("like", if like { "1" } else { "2" }.to_owned()),
                ("csrf", self.csrf()?),
            ],
        )
        .await
    }

    pub async fn add_coin(
        &self,
        aid: u64,
        bvid: &str,
        count: u32,
        also_like: bool,
    ) -> Result<(), ApiError> {
        let csrf = self.csrf()?;
        let referer = (!bvid.is_empty()).then(|| format!("https://www.bilibili.com/video/{bvid}"));
        self.post_form_ack_with_referer(
            COIN_URL,
            referer.as_deref(),
            &[
                ("aid", aid.to_string()),
                ("multiply", count.to_string()),
                ("select_like", if also_like { "1" } else { "0" }.to_owned()),
                ("cross_domain", "true".to_owned()),
                ("from_spmid", WEB_SPMID.to_owned()),
                ("spmid", WEB_SPMID.to_owned()),
                ("statistics", WEB_STATISTICS.to_owned()),
                ("eab_x", "1".to_owned()),
                ("ramval", ramval().to_string()),
                ("source", "web_normal".to_owned()),
                ("ga", "1".to_owned()),
                ("csrf", csrf.clone()),
                ("csrf_token", csrf),
            ],
        )
        .await
    }

    pub async fn set_favorite(
        &self,
        aid: u64,
        folder_id: u64,
        favorite: bool,
    ) -> Result<(), ApiError> {
        let folder = folder_id.to_string();
        self.post_form_ack(
            FAVORITE_URL,
            &[
                ("rid", aid.to_string()),
                ("type", "2".to_owned()),
                (
                    "add_media_ids",
                    if favorite {
                        folder.clone()
                    } else {
                        String::new()
                    },
                ),
                (
                    "del_media_ids",
                    if favorite { String::new() } else { folder },
                ),
                ("csrf", self.csrf()?),
            ],
        )
        .await
    }
}

fn ramval() -> u64 {
    unix_timestamp() % 9_000_000 + 1_000_000
}
