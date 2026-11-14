use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::*;
use rigili_bilibili::{
    ApiError, ArchiveRelation, ArticleSearch, BangumiFollows, Chapter, Client, CommentPage,
    CreatedFolder, Dynamics, FavFolders, FavItems, HistoryPage, HotWords, LiveRoom, LiveSearch,
    MediaSearch, MessageCursor, MessageFeed, MessageSessions, MessageUserInfo, Nav, NavStat,
    PgcIndex, PgcSeason, PlayUrl, Popular, QrCode, QrPoll, Recommend, SearchAll, SearchSuggestions,
    SessionError, SpaceCollections, SpaceProfile, SpaceRelationStatus, SpaceVideos, UserSearch,
    VideoDetail, VideoInfo, VideoSearch, VideoShot, WatchLater, api,
    api::comment::CommentSort,
    api::message::MessageFeedKind,
    api::search::{SearchOrder, SearchType},
    api::space::SpaceVideoOrder,
};

#[derive(Clone)]
pub struct Backend {
    client: Arc<Client>,
}

enum BackendState {
    Ready(Backend),
    Failed(ApiError),
}

impl Global for BackendState {}

pub fn init(cx: &mut App) {
    let state = match Backend::new() {
        Ok(backend) => BackendState::Ready(backend),
        Err(error) => BackendState::Failed(error),
    };
    cx.set_global(state);
}

pub fn get(cx: &App) -> Result<Backend, &ApiError> {
    match cx.global::<BackendState>() {
        BackendState::Ready(backend) => Ok(backend.clone()),
        BackendState::Failed(error) => Err(error),
    }
}

pub fn cached(slot: &mut Option<Backend>, cx: &App) -> Result<Backend, String> {
    if let Some(backend) = slot {
        return Ok(backend.clone());
    }
    match get(cx) {
        Ok(backend) => {
            *slot = Some(backend.clone());
            Ok(backend)
        }
        Err(error) => Err(crate::api_error_message(error)),
    }
}

impl Backend {
    fn new() -> Result<Self, ApiError> {
        Ok(Self {
            client: Arc::new(Client::new()?),
        })
    }

    pub async fn popular(&self, page: u32, page_size: u32) -> Result<Popular, ApiError> {
        api::popular::fetch(&self.client, page, page_size).await
    }

    pub async fn recommend(&self, fresh_idx: u32, page_size: u32) -> Result<Recommend, ApiError> {
        api::recommend::fetch(&self.client, fresh_idx, page_size).await
    }

    pub async fn live_list(&self, page: u32, page_size: u32) -> Result<Vec<LiveRoom>, ApiError> {
        api::live::list(&self.client, page, page_size).await
    }

    pub async fn live_play_url(&self, room_id: u64) -> Result<String, ApiError> {
        api::live::play_url(&self.client, room_id).await
    }

    pub async fn pgc_index(
        &self,
        season_type: u32,
        page: u32,
        page_size: u32,
    ) -> Result<PgcIndex, ApiError> {
        api::pgc::index(&self.client, season_type, page, page_size).await
    }

    pub async fn pgc_season(&self, season_id: u64) -> Result<PgcSeason, ApiError> {
        api::pgc::season(&self.client, season_id).await
    }

    pub async fn search_videos(
        &self,
        keyword: &str,
        page: u32,
        order: SearchOrder,
    ) -> Result<VideoSearch, ApiError> {
        api::search::fetch_videos(&self.client, keyword, page, order).await
    }

    pub async fn search_users(
        &self,
        keyword: &str,
        page: u32,
        order: SearchOrder,
    ) -> Result<UserSearch, ApiError> {
        api::search::fetch_users(&self.client, keyword, page, order).await
    }

    pub async fn search_live(
        &self,
        keyword: &str,
        page: u32,
        order: SearchOrder,
    ) -> Result<LiveSearch, ApiError> {
        api::search::fetch_live(&self.client, keyword, page, order).await
    }

    pub async fn search_media(
        &self,
        keyword: &str,
        page: u32,
        order: SearchOrder,
        search_type: SearchType,
    ) -> Result<MediaSearch, ApiError> {
        api::search::fetch_media(&self.client, keyword, page, order, search_type).await
    }

    pub async fn search_articles(
        &self,
        keyword: &str,
        page: u32,
        order: SearchOrder,
    ) -> Result<ArticleSearch, ApiError> {
        api::search::fetch_articles(&self.client, keyword, page, order).await
    }

    pub async fn search_all(&self, keyword: &str) -> Result<SearchAll, ApiError> {
        api::search::fetch_all(&self.client, keyword).await
    }

    pub async fn search_suggestions(&self, term: &str) -> Result<SearchSuggestions, ApiError> {
        api::search::fetch_suggestions(&self.client, term).await
    }

    pub async fn hot_words(&self) -> Result<HotWords, ApiError> {
        api::search::fetch_hot_words(&self.client).await
    }

    pub async fn nav_stat(&self) -> Result<NavStat, ApiError> {
        api::mine::nav_stat(&self.client).await
    }

    pub async fn fav_folders(&self, mid: u64) -> Result<FavFolders, ApiError> {
        api::mine::fav_folders(&self.client, mid).await
    }

    pub async fn fav_folders_with_state(&self, mid: u64, rid: u64) -> Result<FavFolders, ApiError> {
        api::mine::fav_folders_with_state(&self.client, mid, rid).await
    }

    pub async fn collected_folders(&self, mid: u64) -> Result<FavFolders, ApiError> {
        api::mine::collected_folders(&self.client, mid).await
    }

    pub async fn create_folder(
        &self,
        title: &str,
        private: bool,
    ) -> Result<CreatedFolder, ApiError> {
        api::mine::create_folder(&self.client, title, private).await
    }

    pub async fn sort_folders(&self, ids: &[u64]) -> Result<(), ApiError> {
        api::mine::sort_folders(&self.client, ids).await
    }

    pub async fn bangumi_follows(
        &self,
        mid: u64,
        season_type: u32,
        page: u32,
    ) -> Result<BangumiFollows, ApiError> {
        api::mine::bangumi_follows(&self.client, mid, season_type, page).await
    }

    pub async fn space_profile(&self, mid: u64) -> Result<SpaceProfile, ApiError> {
        api::space::profile(&self.client, mid).await
    }

    pub async fn modify_relation(&self, mid: u64, follow: bool) -> Result<(), ApiError> {
        api::space::modify_relation(&self.client, mid, follow).await
    }

    pub async fn relation_status(&self, mid: u64) -> Result<SpaceRelationStatus, ApiError> {
        api::space::relation_status(&self.client, mid).await
    }

    pub async fn relation_statuses(
        &self,
        mids: &[u64],
    ) -> Result<BTreeMap<u64, SpaceRelationStatus>, ApiError> {
        api::space::relation_statuses(&self.client, mids).await
    }

    pub async fn comments(
        &self,
        kind: u32,
        oid: &str,
        page: u32,
        sort: CommentSort,
    ) -> Result<CommentPage, ApiError> {
        api::comment::fetch(&self.client, kind, oid, page, sort).await
    }

    pub async fn add_comment(&self, aid: u64, message: &str) -> Result<(), ApiError> {
        api::comment::add(&self.client, 1, &aid.to_string(), message).await
    }

    pub async fn dynamics(&self, mid: u64, offset: &str) -> Result<Dynamics, ApiError> {
        api::space::dynamics(&self.client, mid, offset).await
    }

    pub async fn space_videos(
        &self,
        mid: u64,
        page: u32,
        order: SpaceVideoOrder,
        keyword: &str,
    ) -> Result<SpaceVideos, ApiError> {
        api::space::videos(&self.client, mid, page, order, keyword).await
    }

    pub async fn space_preview_videos(&self, mid: u64) -> Result<SpaceVideos, ApiError> {
        api::space::preview_videos(&self.client, mid).await
    }

    pub async fn collections(&self, mid: u64, page: u32) -> Result<SpaceCollections, ApiError> {
        api::space::collections(&self.client, mid, page).await
    }

    pub async fn fav_items(
        &self,
        media_id: u64,
        page: u32,
        keyword: &str,
    ) -> Result<FavItems, ApiError> {
        api::mine::fav_items(&self.client, media_id, page, keyword).await
    }

    pub async fn history(&self, max: u64, view_at: i64) -> Result<HistoryPage, ApiError> {
        api::mine::history(&self.client, max, view_at).await
    }

    pub async fn watch_later(&self) -> Result<WatchLater, ApiError> {
        api::mine::watch_later(&self.client).await
    }

    pub async fn watch_later_add(&self, bvid: &str) -> Result<(), ApiError> {
        api::mine::watch_later_add(&self.client, bvid).await
    }

    pub async fn watch_later_remove(&self, aid: u64) -> Result<(), ApiError> {
        api::mine::watch_later_remove(&self.client, aid).await
    }

    pub async fn message_sessions(&self) -> Result<MessageSessions, ApiError> {
        api::message::sessions(&self.client).await
    }

    pub async fn message_user_infos(
        &self,
        uids: Vec<u64>,
    ) -> Result<Vec<MessageUserInfo>, ApiError> {
        api::message::user_infos(&self.client, &uids).await
    }

    pub async fn message_feed(
        &self,
        kind: MessageFeedKind,
        cursor: Option<MessageCursor>,
    ) -> Result<MessageFeed, ApiError> {
        api::message::feed(&self.client, kind, cursor).await
    }

    pub async fn send_message(&self, receiver: u64, content: &str) -> Result<(), ApiError> {
        api::message::send_message(&self.client, receiver, content).await
    }

    pub async fn view(&self, bvid: &str) -> Result<VideoInfo, ApiError> {
        api::view::fetch(&self.client, bvid).await
    }

    pub async fn videoshot(&self, bvid: &str, cid: u64) -> Result<VideoShot, ApiError> {
        api::view::videoshot(&self.client, bvid, cid).await
    }

    pub async fn video_detail(&self, bvid: &str) -> Result<VideoDetail, ApiError> {
        api::view::detail(&self.client, bvid).await
    }

    pub async fn archive_relation(&self, aid: u64) -> Result<ArchiveRelation, ApiError> {
        api::interact::relation(&self.client, aid).await
    }

    pub async fn set_like(&self, aid: u64, like: bool) -> Result<(), ApiError> {
        api::interact::set_like(&self.client, aid, like).await
    }

    pub async fn add_coin(&self, aid: u64) -> Result<(), ApiError> {
        api::interact::add_coin(&self.client, aid).await
    }

    pub async fn set_favorite(
        &self,
        aid: u64,
        folder_id: u64,
        favorite: bool,
    ) -> Result<(), ApiError> {
        api::interact::set_favorite(&self.client, aid, folder_id, favorite).await
    }

    pub async fn chapters(&self, bvid: &str, cid: u64) -> Result<Vec<Chapter>, ApiError> {
        api::view::chapters(&self.client, bvid, cid).await
    }

    pub async fn playurl(&self, bvid: &str, cid: u64) -> Result<PlayUrl, ApiError> {
        api::playurl::fetch(&self.client, bvid, cid).await
    }

    pub async fn qr_generate(&self) -> Result<QrCode, ApiError> {
        api::login::generate(&self.client).await
    }

    pub async fn qr_poll(&self, qrcode_key: &str) -> Result<QrPoll, ApiError> {
        api::login::poll(&self.client, qrcode_key).await
    }

    pub async fn nav(&self) -> Result<Nav, ApiError> {
        api::login::nav(&self.client).await
    }

    pub fn apply_cookies(&self, cookies: BTreeMap<String, String>) {
        self.client.set_cookies(cookies);
    }

    pub fn cookie_jar_values(&self) -> BTreeMap<String, String> {
        self.client.jar_cookies()
    }

    pub fn save_session(&self) -> Result<(), SessionError> {
        self.client.save_session()
    }

    pub fn has_session(&self) -> bool {
        !self.client.cookie_values().is_empty()
    }
}
