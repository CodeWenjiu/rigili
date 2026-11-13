use crate::{
    ApiError, BangumiFollows, Client, CreatedFolder, FavFolders, FavItems, HistoryPage, NavStat,
    WatchLater,
};

const NAV_STAT_URL: &str = "https://api.bilibili.com/x/web-interface/nav/stat";
const FAV_FOLDERS_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/created/list";
const COLLECTED_FOLDERS_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/collected/list";
const FAV_ITEMS_URL: &str = "https://api.bilibili.com/x/v3/fav/resource/list";
const FOLDER_ADD_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/add";
const FOLDER_SORT_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/sort";
const BANGUMI_FOLLOW_URL: &str = "https://api.bilibili.com/x/space/bangumi/follow/list";
const HISTORY_URL: &str = "https://api.bilibili.com/x/web-interface/history/cursor";
const WATCH_LATER_URL: &str = "https://api.bilibili.com/x/v2/history/toview";
const WATCH_LATER_ADD_URL: &str = "https://api.bilibili.com/x/v2/history/toview/add";
const WATCH_LATER_DEL_URL: &str = "https://api.bilibili.com/x/v2/history/toview/del";
const PAGE_SIZE: u32 = 20;
const FOLDER_LIMIT: u32 = 100;

pub async fn nav_stat(client: &Client) -> Result<NavStat, ApiError> {
    client.get_json(NAV_STAT_URL, &[]).await
}

pub async fn fav_folders(client: &Client, mid: u64) -> Result<FavFolders, ApiError> {
    client
        .get_json(
            FAV_FOLDERS_URL,
            &[
                ("up_mid", mid.to_string()),
                ("pn", 1.to_string()),
                ("ps", FOLDER_LIMIT.to_string()),
            ],
        )
        .await
}

pub async fn collected_folders(client: &Client, mid: u64) -> Result<FavFolders, ApiError> {
    client
        .get_json(
            COLLECTED_FOLDERS_URL,
            &[
                ("up_mid", mid.to_string()),
                ("pn", 1.to_string()),
                ("ps", FOLDER_LIMIT.to_string()),
                ("platform", "web".into()),
            ],
        )
        .await
}

pub async fn fav_items(
    client: &Client,
    media_id: u64,
    page: u32,
    keyword: &str,
) -> Result<FavItems, ApiError> {
    let page = page.max(1);

    client
        .get_json(
            FAV_ITEMS_URL,
            &[
                ("media_id", media_id.to_string()),
                ("pn", page.to_string()),
                ("ps", PAGE_SIZE.to_string()),
                ("keyword", keyword.to_owned()),
                ("platform", "web".into()),
            ],
        )
        .await
}

pub async fn create_folder(
    client: &Client,
    title: &str,
    private: bool,
) -> Result<CreatedFolder, ApiError> {
    client
        .post_form(
            FOLDER_ADD_URL,
            &[
                ("title", title.to_owned()),
                ("intro", String::new()),
                ("privacy", if private { "1" } else { "0" }.to_owned()),
                ("csrf", client.csrf()?),
            ],
        )
        .await
}

pub async fn sort_folders(client: &Client, ids: &[u64]) -> Result<(), ApiError> {
    if ids.is_empty() {
        return Ok(());
    }

    let sort = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    client
        .post_form::<serde_json::Value>(
            FOLDER_SORT_URL,
            &[
                ("sort", sort),
                ("csrf", client.csrf()?),
                ("platform", "web".into()),
            ],
        )
        .await
        .map(|_| ())
}

pub async fn bangumi_follows(
    client: &Client,
    mid: u64,
    season_type: u32,
    page: u32,
) -> Result<BangumiFollows, ApiError> {
    client
        .get_json(
            BANGUMI_FOLLOW_URL,
            &[
                ("type", season_type.to_string()),
                ("follow_status", 0.to_string()),
                ("pn", page.max(1).to_string()),
                ("ps", PAGE_SIZE.to_string()),
                ("vmid", mid.to_string()),
            ],
        )
        .await
}

pub async fn history(client: &Client, max: u64, view_at: i64) -> Result<HistoryPage, ApiError> {
    client
        .get_json(
            HISTORY_URL,
            &[
                ("max", max.to_string()),
                ("view_at", view_at.to_string()),
                ("business", String::new()),
            ],
        )
        .await
}

pub async fn watch_later(client: &Client) -> Result<WatchLater, ApiError> {
    client.get_json(WATCH_LATER_URL, &[]).await
}

pub async fn watch_later_add(client: &Client, bvid: &str) -> Result<(), ApiError> {
    client
        .post_form_ack(
            WATCH_LATER_ADD_URL,
            &[("bvid", bvid.to_owned()), ("csrf", client.csrf()?)],
        )
        .await
}

pub async fn watch_later_remove(client: &Client, aid: u64) -> Result<(), ApiError> {
    client
        .post_form_ack(
            WATCH_LATER_DEL_URL,
            &[("aid", aid.to_string()), ("csrf", client.csrf()?)],
        )
        .await
}
