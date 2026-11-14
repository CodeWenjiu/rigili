use crate::{ApiError, ArchiveRelation, Client};

const RELATION_URL: &str = "https://api.bilibili.com/x/web-interface/archive/relation";
const LIKE_URL: &str = "https://api.bilibili.com/x/web-interface/archive/like";
const COIN_URL: &str = "https://api.bilibili.com/x/web-interface/coin/add";
const FAVORITE_URL: &str = "https://api.bilibili.com/x/v3/fav/resource/deal";

pub async fn relation(client: &Client, aid: u64) -> Result<ArchiveRelation, ApiError> {
    client
        .get_json(RELATION_URL, &[("aid", aid.to_string())])
        .await
}

pub async fn set_like(client: &Client, aid: u64, like: bool) -> Result<(), ApiError> {
    client
        .post_form_ack(
            LIKE_URL,
            &[
                ("aid", aid.to_string()),
                ("like", if like { "1" } else { "2" }.to_owned()),
                ("csrf", client.csrf()?),
            ],
        )
        .await
}

pub async fn add_coin(client: &Client, aid: u64) -> Result<(), ApiError> {
    client
        .post_form_ack(
            COIN_URL,
            &[
                ("aid", aid.to_string()),
                ("multiply", "1".to_owned()),
                ("select_like", "0".to_owned()),
                ("csrf", client.csrf()?),
            ],
        )
        .await
}

pub async fn set_favorite(
    client: &Client,
    aid: u64,
    folder_id: u64,
    favorite: bool,
) -> Result<(), ApiError> {
    let folder = folder_id.to_string();
    client
        .post_form_ack(
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
                ("csrf", client.csrf()?),
            ],
        )
        .await
}
