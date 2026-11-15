use gpui_kit::*;

use super::{FavoriteView, FolderSource, ListMeta, MineTab, MineView, VideoListState};
use rigili_bilibili::{BangumiFollow, BangumiFollows};
use rigili_core::api_task_error_message;
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::spawn_api;

use crate::shared::browse::grid::VideoCard;
use crate::shared::browse::paging::{Page, spawn_first, spawn_more};

const BANGUMI_PAGE_SIZE: usize = 20;

impl MineView {
    pub(super) fn backend(&mut self, cx: &mut Context<Self>) -> Result<Backend, String> {
        backend::cached(&mut self.backend, cx)
    }

    pub(super) fn load_stat(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none()
            || self.stat.is_some()
            || self.stat_loading
            || self.stat_error.is_some()
        {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.stat_error = Some(error);
                cx.notify();
                return;
            }
        };

        self.stat_loading = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.nav_stat().await },
            move |this, result, cx| {
                this.stat_loading = false;
                match result {
                    Ok(stat) => this.stat = Some(stat),
                    Err(error) => this.stat_error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none() {
            return;
        }
        match self.tab {
            MineTab::Favorites => {
                if !self.folders.loaded && !self.folders.loading && self.folders.error.is_none() {
                    self.load_folders(cx);
                }
                self.load_favorite_view(cx);
            }
            MineTab::History => {
                if !self.history.loaded && !self.history.loading && self.history.error.is_none() {
                    self.load_history(cx);
                }
            }
            MineTab::WatchLater => {
                if !self.watch_later.loaded
                    && !self.watch_later.loading
                    && self.watch_later.error.is_none()
                {
                    self.load_watch_later(cx);
                }
            }
            MineTab::Offline => {}
        }
    }

    pub(super) fn reload_tab(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        match tab {
            MineTab::Favorites => self.load_folders(cx),
            MineTab::History => self.load_history(cx),
            MineTab::WatchLater => self.load_watch_later(cx),
            MineTab::Offline => {}
        }
    }

    pub(super) fn load_folders(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
            return;
        };
        if self.folders.loading {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.folders.error = Some(error);
                cx.notify();
                return;
            }
        };

        self.folders.generation += 1;
        let generation = self.folders.generation;
        self.folders.loading = true;
        self.folders.loaded = false;
        self.folders.error = None;
        self.folders.folders.clear();
        self.folders.current = None;
        self.favorites = VideoListState::new();
        cx.notify();

        spawn_api(
            cx,
            async move { backend.fav_folders(mid).await },
            move |this, result, cx| {
                if this.folders.generation != generation {
                    return;
                }
                this.folders.loading = false;
                match result {
                    Ok(page) => {
                        this.folders.folders = page.into_items();
                        this.folders.loaded = true;
                        this.folders.current = this.default_folder_id();
                        if this.folders.current.is_some() {
                            this.load_favorite_items(cx);
                        }
                    }
                    Err(error) => this.folders.error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn open_folder(&mut self, id: u64, source: FolderSource, cx: &mut Context<Self>) {
        self.browsing_folder = Some(match source {
            FolderSource::Created => FavoriteView::Folders,
            FolderSource::Collected => FavoriteView::Subscribed,
        });
        if self.folders.current != Some(id) {
            self.folders.current = Some(id);
            self.load_favorite_items(cx);
        } else if self.favorites_keyword != self.search.keyword() {
            self.load_favorite_items(cx);
        }
        cx.notify();
    }

    pub(super) fn load_favorite_items(&mut self, cx: &mut Context<Self>) {
        let Some(media_id) = self.folders.current else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.favorites.fail_first_reset(error);
                cx.notify();
                return;
            }
        };
        let keyword = self.search.keyword().to_owned();
        self.favorites_keyword = keyword.clone();

        spawn_first(
            self,
            cx,
            |this| &mut this.favorites.list,
            |card: &VideoCard| card.bvid().to_owned(),
            async move {
                let page = backend.fav_items(media_id, 1, &keyword).await?;
                let has_more = page.has_more;
                Ok(Page::new(
                    favorite_cards(page.into_items()),
                    has_more,
                    ListMeta {
                        page: 1,
                        ..Default::default()
                    },
                ))
            },
            |this| {
                this.favorites.scroll.scroll_to_item(0, ScrollStrategy::Top);
            },
        );
    }

    pub(super) fn load_more_favorites(&mut self, cx: &mut Context<Self>) {
        let Some(media_id) = self.folders.current else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.favorites.fail_more(error);
                cx.notify();
                return;
            }
        };
        let keyword = self.search.keyword().to_owned();

        spawn_more(
            self,
            cx,
            |this| &mut this.favorites.list,
            |card: &VideoCard| card.bvid().to_owned(),
            move |meta| {
                let page = meta.page + 1;
                async move {
                    let result = backend.fav_items(media_id, page, &keyword).await?;
                    let has_more = result.has_more;
                    Ok(Page::new(
                        favorite_cards(result.into_items()),
                        has_more,
                        ListMeta {
                            page,
                            ..Default::default()
                        },
                    ))
                }
            },
            |_| {},
        );
    }

    pub(super) fn load_history(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none() {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.history.fail_first_reset(error);
                cx.notify();
                return;
            }
        };

        spawn_first(
            self,
            cx,
            |this| &mut this.history.list,
            |card: &VideoCard| card.bvid().to_owned(),
            async move {
                let page = backend.history(0, 0).await?;
                Ok(history_page(page))
            },
            |this| {
                this.history.scroll.scroll_to_item(0, ScrollStrategy::Top);
            },
        );
    }

    pub(super) fn load_more_history(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none() {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.history.fail_more(error);
                cx.notify();
                return;
            }
        };

        spawn_more(
            self,
            cx,
            |this| &mut this.history.list,
            |card: &VideoCard| card.bvid().to_owned(),
            move |meta| async move {
                let page = backend.history(meta.max, meta.view_at).await?;
                Ok(history_page(page))
            },
            |_| {},
        );
    }

    pub(super) fn load_watch_later(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none() {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.watch_later.fail_first_reset(error);
                cx.notify();
                return;
            }
        };

        spawn_first(
            self,
            cx,
            |this| &mut this.watch_later.list,
            |card: &VideoCard| card.bvid().to_owned(),
            async move {
                let page = backend.watch_later().await?;
                Ok(Page::new(
                    watch_later_cards(page.into_items()),
                    false,
                    ListMeta::default(),
                ))
            },
            |this| {
                this.watch_later
                    .scroll
                    .scroll_to_item(0, ScrollStrategy::Top);
            },
        );
    }

    pub(super) fn retry_more(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        match tab {
            MineTab::Favorites => {
                self.favorites.retry_more();
                self.load_more_favorites(cx);
            }
            MineTab::History => {
                self.history.retry_more();
                self.load_more_history(cx);
            }
            _ => {}
        }
    }

    pub(super) fn maybe_load_more(
        &mut self,
        tab: MineTab,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows == 0 || last_visible + 1 < rows {
            return;
        }
        match tab {
            MineTab::Favorites => self.load_more_favorites(cx),
            MineTab::History => self.load_more_history(cx),
            _ => {}
        }
    }

    pub(super) fn load_collected(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
            return;
        };
        if self.collected.loading {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.collected.error = Some(error);
                cx.notify();
                return;
            }
        };

        self.collected.generation += 1;
        let generation = self.collected.generation;
        self.collected.loading = true;
        self.collected.loaded = false;
        self.collected.error = None;
        self.collected.folders.clear();
        cx.notify();

        spawn_api(
            cx,
            async move { backend.collected_folders(mid).await },
            move |this, result, cx| {
                if this.collected.generation != generation {
                    return;
                }
                this.collected.loading = false;
                match result {
                    Ok(page) => {
                        this.collected.folders = page
                            .into_items()
                            .into_iter()
                            .filter(|folder| folder.is_valid())
                            .collect();
                        this.collected.loaded = true;
                    }
                    Err(error) => {
                        this.collected.error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_bangumi(&mut self, cinema: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.bangumi_state_mut(cinema).0.fail_first_reset(error);
                cx.notify();
                return;
            }
        };
        let season_type = if cinema { 2 } else { 1 };

        spawn_first(
            self,
            cx,
            move |this| &mut this.bangumi_state_mut(cinema).0,
            |item: &BangumiFollow| item.season_id.to_string(),
            async move { bangumi_page(backend.bangumi_follows(mid, season_type, 1).await?, 1) },
            move |this| {
                this.bangumi_state_mut(cinema)
                    .scroll
                    .scroll_to_item(0, ScrollStrategy::Top);
            },
        );
    }

    pub(super) fn load_more_bangumi(&mut self, cinema: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.bangumi_state_mut(cinema).0.fail_more(error);
                cx.notify();
                return;
            }
        };
        let season_type = if cinema { 2 } else { 1 };

        spawn_more(
            self,
            cx,
            move |this| &mut this.bangumi_state_mut(cinema).0,
            |item: &BangumiFollow| item.season_id.to_string(),
            move |meta| {
                let page = meta + 1;
                async move { bangumi_page(backend.bangumi_follows(mid, season_type, page).await?, page) }
            },
            |_| {},
        );
    }

    pub(super) fn retry_bangumi_more(&mut self, cinema: bool, cx: &mut Context<Self>) {
        self.bangumi_state_mut(cinema).retry_more();
        self.load_more_bangumi(cinema, cx);
    }

    pub(super) fn maybe_load_more_bangumi(
        &mut self,
        cinema: bool,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows == 0 || last_visible + 1 < rows {
            return;
        }
        self.load_more_bangumi(cinema, cx);
    }
}

fn history_page(page: rigili_bilibili::HistoryPage) -> Page<VideoCard, ListMeta> {
    let has_more = !page.list.is_empty();
    Page::new(
        history_cards(page.list),
        has_more,
        ListMeta {
            page: 0,
            max: page.cursor.max,
            view_at: page.cursor.view_at,
        },
    )
}

fn bangumi_page(
    page: BangumiFollows,
    number: u32,
) -> Result<Page<BangumiFollow, u32>, rigili_bilibili::ApiError> {
    let has_more = page.list.len() >= BANGUMI_PAGE_SIZE;
    Ok(Page::new(
        page.list.into_iter().filter(|item| item.season_id != 0),
        has_more,
        number,
    ))
}

fn favorite_cards(items: Vec<rigili_bilibili::FavItem>) -> Vec<VideoCard> {
    items
        .into_iter()
        .filter(|item| !item.bvid.is_empty())
        .map(VideoCard::from)
        .collect()
}

fn history_cards(items: Vec<rigili_bilibili::HistoryItem>) -> Vec<VideoCard> {
    items
        .into_iter()
        .filter(|item| !item.history.bvid.is_empty())
        .map(VideoCard::from)
        .collect()
}

fn watch_later_cards(items: Vec<rigili_bilibili::WatchLaterItem>) -> Vec<VideoCard> {
    items
        .into_iter()
        .filter(|item| !item.bvid.is_empty())
        .map(VideoCard::from)
        .collect()
}
