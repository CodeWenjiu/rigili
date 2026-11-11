use gpui_kit::*;

use super::{FavoriteView, FolderSource, MineTab, MineView, VideoListState};
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::spawn_api;

use crate::grid::{VideoCard, append_unique_by};
use rigili_core::api_task_error_message;

const BANGUMI_PAGE_SIZE: usize = 20;

impl MineView {
    pub(super) fn backend(&mut self, cx: &mut Context<Self>) -> Result<Backend, String> {
        if let Some(backend) = &self.backend {
            return Ok(backend.clone());
        }
        match backend::get(cx) {
            Ok(backend) => {
                self.backend = Some(backend.clone());
                Ok(backend)
            }
            Err(error) => Err(error),
        }
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
        if self.space_mid().is_none() {
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
            MineTab::Dynamics | MineTab::Videos | MineTab::Collections => {
                self.load_space_if_needed(cx);
            }
        }
    }

    pub(super) fn reload_tab(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        match tab {
            MineTab::Favorites => self.load_folders(cx),
            MineTab::History => self.load_history(cx),
            MineTab::WatchLater => self.load_watch_later(cx),
            MineTab::Offline => {}
            MineTab::Dynamics | MineTab::Videos | MineTab::Collections => {
                self.reload_space_tab(cx);
            }
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
                self.favorites.reset();
                self.favorites.loading = false;
                self.favorites.loaded = true;
                self.favorites.error = Some(error);
                cx.notify();
                return;
            }
        };

        let generation = self.favorites.reset();
        cx.notify();

        spawn_api(
            cx,
            async move { backend.fav_items(media_id, 1).await },
            move |this, result, cx| {
                if this.favorites.generation != generation {
                    return;
                }
                this.favorites.loading = false;
                this.favorites.loaded = true;
                match result {
                    Ok(page) => {
                        this.favorites.page = 1;
                        this.favorites.has_more = page.has_more;
                        this.favorites.items = favorite_cards(page.into_items());
                        this.favorites.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Err(error) => this.favorites.error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_more_favorites(&mut self, cx: &mut Context<Self>) {
        if self.favorites.loading
            || self.favorites.loading_more
            || !self.favorites.has_more
            || self.favorites.more_error.is_some()
        {
            return;
        }
        let Some(media_id) = self.folders.current else {
            return;
        };
        let page = self.favorites.page + 1;
        let generation = self.favorites.generation;
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.favorites.more_error = Some(error);
                cx.notify();
                return;
            }
        };

        self.favorites.loading_more = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.fav_items(media_id, page).await },
            move |this, result, cx| {
                if this.favorites.generation != generation {
                    return;
                }
                this.favorites.loading_more = false;
                match result {
                    Ok(page_result) => {
                        this.favorites.page = page;
                        this.favorites.has_more = page_result.has_more;
                        append_unique_by(
                            &mut this.favorites.items,
                            favorite_cards(page_result.into_items()),
                            |card| card.bvid().to_owned(),
                        );
                    }
                    Err(error) => {
                        this.favorites.more_error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_history(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none() {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.history.reset();
                self.history.loading = false;
                self.history.loaded = true;
                self.history.error = Some(error);
                cx.notify();
                return;
            }
        };

        let generation = self.history.reset();
        cx.notify();

        spawn_api(
            cx,
            async move { backend.history(0, 0).await },
            move |this, result, cx| {
                if this.history.generation != generation {
                    return;
                }
                this.history.loading = false;
                this.history.loaded = true;
                match result {
                    Ok(page) => {
                        this.history.cursor_max = page.cursor.max;
                        this.history.cursor_view_at = page.cursor.view_at;
                        this.history.has_more = !page.list.is_empty();
                        this.history.items = history_cards(page.list);
                        this.history.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Err(error) => this.history.error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_more_history(&mut self, cx: &mut Context<Self>) {
        if self.history.loading
            || self.history.loading_more
            || !self.history.has_more
            || self.history.more_error.is_some()
        {
            return;
        }
        let max = self.history.cursor_max;
        let view_at = self.history.cursor_view_at;
        let generation = self.history.generation;
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.history.more_error = Some(error);
                cx.notify();
                return;
            }
        };

        self.history.loading_more = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.history(max, view_at).await },
            move |this, result, cx| {
                if this.history.generation != generation {
                    return;
                }
                this.history.loading_more = false;
                match result {
                    Ok(page) => {
                        this.history.cursor_max = page.cursor.max;
                        this.history.cursor_view_at = page.cursor.view_at;
                        this.history.has_more = !page.list.is_empty();
                        append_unique_by(
                            &mut this.history.items,
                            history_cards(page.list),
                            |card| card.bvid().to_owned(),
                        );
                    }
                    Err(error) => {
                        this.history.more_error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_watch_later(&mut self, cx: &mut Context<Self>) {
        if self.user_mid.is_none() {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.watch_later.reset();
                self.watch_later.loading = false;
                self.watch_later.loaded = true;
                self.watch_later.error = Some(error);
                cx.notify();
                return;
            }
        };

        let generation = self.watch_later.reset();
        cx.notify();

        spawn_api(
            cx,
            async move { backend.watch_later().await },
            move |this, result, cx| {
                if this.watch_later.generation != generation {
                    return;
                }
                this.watch_later.loading = false;
                this.watch_later.loaded = true;
                match result {
                    Ok(page) => {
                        this.watch_later.items = watch_later_cards(page.into_items());
                        this.watch_later
                            .scroll
                            .scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Err(error) => {
                        this.watch_later.error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn retry_more(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        match tab {
            MineTab::Favorites => {
                self.favorites.more_error = None;
                self.load_more_favorites(cx);
            }
            MineTab::History => {
                self.history.more_error = None;
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
                let state = self.bangumi_state_mut(cinema);
                state.reset();
                state.loading = false;
                state.loaded = true;
                state.error = Some(error);
                cx.notify();
                return;
            }
        };
        let season_type = if cinema { 2 } else { 1 };
        let generation = self.bangumi_state_mut(cinema).reset();
        cx.notify();

        spawn_api(
            cx,
            async move { backend.bangumi_follows(mid, season_type, 1).await },
            move |this, result, cx| {
                if this.bangumi_state(cinema).generation != generation {
                    return;
                }
                let state = this.bangumi_state_mut(cinema);
                state.loading = false;
                state.loaded = true;
                match result {
                    Ok(page) => {
                        state.page = 1;
                        state.has_more = page.list.len() >= BANGUMI_PAGE_SIZE;
                        state.items = page
                            .list
                            .into_iter()
                            .filter(|item| item.season_id != 0)
                            .collect();
                        state.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Err(error) => {
                        state.error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn load_more_bangumi(&mut self, cinema: bool, cx: &mut Context<Self>) {
        {
            let state = self.bangumi_state(cinema);
            if state.loading || state.loading_more || !state.has_more || state.more_error.is_some()
            {
                return;
            }
        }
        let Some(mid) = self.user_mid else {
            return;
        };
        let page_number = self.bangumi_state(cinema).page + 1;
        let generation = self.bangumi_state(cinema).generation;
        let season_type = if cinema { 2 } else { 1 };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.bangumi_state_mut(cinema).more_error = Some(error);
                cx.notify();
                return;
            }
        };

        self.bangumi_state_mut(cinema).loading_more = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.bangumi_follows(mid, season_type, page_number).await },
            move |this, result, cx| {
                if this.bangumi_state(cinema).generation != generation {
                    return;
                }
                let state = this.bangumi_state_mut(cinema);
                state.loading_more = false;
                match result {
                    Ok(page) => {
                        state.page = page_number;
                        state.has_more = page.list.len() >= BANGUMI_PAGE_SIZE;
                        state
                            .items
                            .extend(page.list.into_iter().filter(|item| item.season_id != 0));
                    }
                    Err(error) => {
                        state.more_error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn retry_bangumi_more(&mut self, cinema: bool, cx: &mut Context<Self>) {
        self.bangumi_state_mut(cinema).more_error = None;
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
