use std::collections::HashSet;

use gpui_kit::*;
use rigili_bilibili::ApiError;
use rigili_bilibili::api::search::{SearchOrder, SearchType};

use super::items::{SearchItem, UserItem};
use super::{SearchTab, SearchView};
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::{ApiTaskError, spawn_api};

use crate::grid::{VideoCard, append_unique_by, index_by};
use rigili_core::{api_error_message, api_task_error_message};

struct Page {
    items: Vec<SearchItem>,
    num_pages: u32,
}

impl Page {
    fn new(items: impl IntoIterator<Item = SearchItem>, num_pages: u32) -> Self {
        Self {
            items: items.into_iter().collect(),
            num_pages,
        }
    }
}

impl SearchView {
    pub(super) fn load_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        if self.keyword.is_empty() {
            return;
        }
        if self.backend.is_none() {
            match backend::get(cx) {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    let state = self.state_mut(tab);
                    state.loading = false;
                    state.error = Some(api_error_message(error));
                    cx.notify();
                    return;
                }
            }
        }

        let Some(backend) = self.backend.clone() else {
            return;
        };
        let keyword = self.keyword.clone();
        let order = self.order;
        let generation = {
            let state = self.state_mut(tab);
            state.generation += 1;
            state.items.clear();
            state.seen.clear();
            state.users.clear();
            state.page = 0;
            state.num_pages = 0;
            state.loading = true;
            state.loading_more = false;
            state.error = None;
            state.more_error = None;
            state.loaded = false;
            state.generation
        };
        cx.notify();

        if tab == SearchTab::All {
            let header_backend = backend.clone();
            let preview_backend = backend.clone();
            let header_keyword = keyword.clone();
            spawn_api(
                cx,
                async move { header_backend.search_all(&header_keyword).await },
                move |this, result, cx| {
                    if this.state(SearchTab::All).generation != generation {
                        return;
                    }
                    if let Ok(all) = result {
                        this.counts = all
                            .pageinfo
                            .into_iter()
                            .map(|(key, info)| (key, info.num_results))
                            .collect();
                        let users = all.users.into_iter().map(UserItem::from).collect();
                        this.state_mut(SearchTab::All).users = users;

                        let mids = this
                            .state(SearchTab::All)
                            .users
                            .first()
                            .map(|user| vec![user.mid])
                            .unwrap_or_default();
                        this.sync_follows(mids, cx);

                        if let Some(mid) = this
                            .state(SearchTab::All)
                            .users
                            .first()
                            .map(|user| user.mid)
                        {
                            let backend = preview_backend.clone();
                            spawn_api(
                                cx,
                                async move { backend.space_preview_videos(mid).await },
                                move |this, result, cx| {
                                    if this.state(SearchTab::All).generation != generation {
                                        return;
                                    }
                                    let Ok(videos) = result else {
                                        return;
                                    };
                                    let incoming = videos
                                        .list
                                        .vlist
                                        .into_iter()
                                        .map(VideoCard::from)
                                        .collect::<Vec<_>>();
                                    let Some(user) =
                                        this.state_mut(SearchTab::All).users.first_mut()
                                    else {
                                        return;
                                    };
                                    let mut seen = HashSet::new();
                                    let mut preview = Vec::new();
                                    for card in incoming
                                        .into_iter()
                                        .chain(std::mem::take(&mut user.preview))
                                    {
                                        if seen.insert(card.bvid().to_owned()) {
                                            preview.push(card);
                                        }
                                    }
                                    user.preview = preview;
                                    cx.notify();
                                },
                            );
                        }
                    }
                    cx.notify();
                },
            );
        }

        let fetch_backend = backend.clone();
        let fetch_keyword = keyword.clone();
        spawn_api(
            cx,
            async move { fetch_page(fetch_backend, tab, fetch_keyword, 1, order).await },
            move |this, result, cx| this.finish_page(tab, 1, generation, result, cx),
        );
    }

    fn finish_page(
        &mut self,
        tab: SearchTab,
        page_number: u32,
        generation: u64,
        result: Result<Page, ApiTaskError>,
        cx: &mut Context<Self>,
    ) {
        if self.state(tab).generation != generation {
            return;
        }

        let state = self.state_mut(tab);
        state.loading = false;
        match result {
            Ok(page) => {
                state.page = page_number;
                state.num_pages = page.num_pages;
                state.items = page.items;
                state.seen = index_by(&state.items, SearchItem::key);
                state.loaded = true;
                state.scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            Err(error) => state.error = Some(api_task_error_message(error, cx)),
        }
        cx.notify();
    }

    pub(super) fn load_more(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        let (page, generation) = {
            let state = self.state(tab);
            if state.loading
                || state.loading_more
                || !has_more(state.page, state.num_pages)
                || state.more_error.is_some()
            {
                return;
            }
            (state.page + 1, state.generation)
        };
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let keyword = self.keyword.clone();
        let order = self.order;
        self.state_mut(tab).loading_more = true;
        cx.notify();

        spawn_api(
            cx,
            async move { fetch_page(backend, tab, keyword, page, order).await },
            move |this, result, cx| {
                if this.state(tab).generation != generation {
                    return;
                }
                let state = this.state_mut(tab);
                state.loading_more = false;
                match result {
                    Ok(fetched) => {
                        state.page = page;
                        state.num_pages = fetched.num_pages;
                        append_unique_by(
                            &mut state.items,
                            &mut state.seen,
                            fetched.items,
                            SearchItem::key,
                        );
                    }
                    Err(error) => state.more_error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn retry_more(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        self.state_mut(tab).more_error = None;
        self.load_more(tab, cx);
    }

    pub(super) fn maybe_load_more(
        &mut self,
        tab: SearchTab,
        row: usize,
        rows: usize,
        header_offset: usize,
        cx: &mut Context<Self>,
    ) {
        if rows > 0 && row.saturating_sub(header_offset) + 1 >= rows {
            self.load_more(tab, cx);
        }
    }

    pub(super) fn open_season(&mut self, season_id: u64, cx: &mut Context<Self>) {
        if self.opening.is_some() {
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };

        self.opening = Some(season_id);
        self.notice = None;
        cx.notify();

        spawn_api(
            cx,
            async move { rigili_core::resolve_season(backend, season_id).await },
            move |this, result, cx| {
                this.opening = None;
                match result {
                    Ok(Some(video)) => cx.emit(video),
                    Ok(None) => this.notice = Some("该作品暂无可播放的剧集".into()),
                    Err(error) => this.notice = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }
}

async fn fetch_page(
    backend: Backend,
    tab: SearchTab,
    keyword: String,
    page: u32,
    order: SearchOrder,
) -> Result<Page, ApiError> {
    match tab {
        SearchTab::All | SearchTab::Video => {
            let result = backend.search_videos(&keyword, page, order).await?;
            Ok(Page::new(
                result
                    .videos
                    .into_iter()
                    .map(|video| SearchItem::Video(VideoCard::from(video))),
                result.num_pages,
            ))
        }
        SearchTab::Bangumi | SearchTab::Film => {
            let search_type = if tab == SearchTab::Film {
                SearchType::Film
            } else {
                SearchType::Bangumi
            };
            let result = backend
                .search_media(&keyword, page, order, search_type)
                .await?;
            Ok(Page::new(
                result
                    .items
                    .into_iter()
                    .map(|item| SearchItem::Media(item.into())),
                result.num_pages,
            ))
        }
        SearchTab::Live => {
            let result = backend.search_live(&keyword, page, order).await?;
            Ok(Page::new(
                result
                    .rooms
                    .into_iter()
                    .map(|room| SearchItem::Live(room.into())),
                result.num_pages,
            ))
        }
        SearchTab::Article => {
            let result = backend.search_articles(&keyword, page, order).await?;
            Ok(Page::new(
                result
                    .articles
                    .into_iter()
                    .map(|article| SearchItem::Article(article.into())),
                result.num_pages,
            ))
        }
        SearchTab::User => {
            let result = backend.search_users(&keyword, page, order).await?;
            Ok(Page::new(
                result
                    .users
                    .into_iter()
                    .map(|user| SearchItem::User(user.into())),
                result.num_pages,
            ))
        }
    }
}

fn has_more(page: u32, num_pages: u32) -> bool {
    page > 0 && page < num_pages
}

#[cfg(test)]
mod tests {
    use super::has_more;

    #[test]
    fn stops_after_the_last_page() {
        assert!(!has_more(0, 10));
        assert!(has_more(1, 10));
        assert!(has_more(9, 10));
        assert!(!has_more(10, 10));
        assert!(!has_more(11, 10));
        assert!(!has_more(1, 0));
    }
}
