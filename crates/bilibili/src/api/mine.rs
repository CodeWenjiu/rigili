use crate::{ApiError, Client, FavFolders, FavItems, HistoryPage, NavStat, WatchLater};

const NAV_STAT_URL: &str = "https://api.bilibili.com/x/web-interface/nav/stat";
const FAV_FOLDERS_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/created/list";
const FAV_ITEMS_URL: &str = "https://api.bilibili.com/x/v3/fav/resource/list";
const HISTORY_URL: &str = "https://api.bilibili.com/x/web-interface/history/cursor";
const WATCH_LATER_URL: &str = "https://api.bilibili.com/x/v2/history/toview";
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

pub async fn fav_items(client: &Client, media_id: u64, page: u32) -> Result<FavItems, ApiError> {
    let page = page.max(1);

    client
        .get_json(
            FAV_ITEMS_URL,
            &[
                ("media_id", media_id.to_string()),
                ("pn", page.to_string()),
                ("ps", PAGE_SIZE.to_string()),
                ("platform", "web".into()),
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
