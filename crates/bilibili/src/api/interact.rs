use crate::{ApiError, ArchiveRelation, Client};

const RELATION_URL: &str = "https://api.bilibili.com/x/web-interface/archive/relation";
const LIKE_URL: &str = "https://api.bilibili.com/x/web-interface/archive/like";
const COIN_URL: &str = "https://api.bilibili.com/x/web-interface/coin/add";
const FAVORITE_URL: &str = "https://api.bilibili.com/x/v3/fav/resource/deal";

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

    pub async fn add_coin(&self, aid: u64, count: u32, also_like: bool) -> Result<(), ApiError> {
        self.post_form_ack(
            COIN_URL,
            &[
                ("aid", aid.to_string()),
                ("multiply", count.to_string()),
                ("select_like", if also_like { "1" } else { "0" }.to_owned()),
                ("csrf", self.csrf()?),
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
