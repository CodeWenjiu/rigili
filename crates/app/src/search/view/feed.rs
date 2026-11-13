use gpui_kit::*;
use rigili_bilibili::ApiError;
use rigili_bilibili::api::search::{SearchOrder, SearchType};

use super::items::{SearchItem, UserItem};
use super::{SearchTab, SearchView};
use rigili_core::api_error_message;
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::spawn_api;

use crate::grid::VideoCard;
use crate::paging::{Page, dedupe_by, spawn_first, spawn_more};

impl SearchView {
    pub(super) fn load_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        if self.keyword.is_empty() {
            return;
        }
        if self.backend.is_none() {
            match backend::get(cx) {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    self.state_mut(tab).fail_first(api_error_message(error));
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
        let all_tab = tab == SearchTab::All;
        let header = all_tab.then(|| (backend.clone(), keyword.clone()));
        if all_tab {
            self.state_mut(tab).users.clear();
        }

        let generation = spawn_first(
            self,
            cx,
            move |this| this.state_mut(tab),
            SearchItem::key,
            async move { fetch_page(backend, tab, keyword, 1, order).await },
            move |this| {
                this.state_mut(tab)
                    .scroll
                    .scroll_to_item(0, ScrollStrategy::Top);
            },
        );

        if let Some((backend, keyword)) = header {
            spawn_header(cx, generation, backend, keyword);
        }
    }

    pub(super) fn load_more(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let keyword = self.keyword.clone();
        let order = self.order;

        spawn_more(
            self,
            cx,
            move |this| this.state_mut(tab),
            SearchItem::key,
            move |page| {
                let page = page + 1;
                async move { fetch_page(backend, tab, keyword, page, order).await }
            },
            |_| {},
        );
    }

    pub(super) fn retry_more(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        self.state_mut(tab).retry_more();
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
        crate::season::SeasonOpener::open_season(self, season_id, cx);
    }
}

fn spawn_header(cx: &mut Context<SearchView>, generation: u64, backend: Backend, keyword: String) {
    let preview_backend = backend.clone();
    spawn_api(
        cx,
        async move { backend.search_all(&keyword).await },
        move |this, result, cx| {
            if !this.state(SearchTab::All).is_current(generation) {
                return;
            }
            let Ok(all) = result else {
                return;
            };

            this.counts = all
                .pageinfo
                .into_iter()
                .map(|(key, info)| (key, info.num_results))
                .collect();
            this.state_mut(SearchTab::All).users =
                all.users.into_iter().map(UserItem::from).collect();

            let first_mid = this
                .state(SearchTab::All)
                .users
                .first()
                .map(|user| user.mid);
            if let Some(mid) = first_mid {
                this.sync_follows(vec![mid], cx);
                let backend = preview_backend.clone();
                spawn_api(
                    cx,
                    async move { backend.space_preview_videos(mid).await },
                    move |this, result, cx| {
                        if !this.state(SearchTab::All).is_current(generation) {
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
                        let Some(user) = this.state_mut(SearchTab::All).users.first_mut() else {
                            return;
                        };
                        user.preview = dedupe_by(
                            incoming
                                .into_iter()
                                .chain(std::mem::take(&mut user.preview)),
                            |card| card.bvid().to_owned(),
                        );
                        cx.notify();
                    },
                );
            }
            cx.notify();
        },
    );
}

async fn fetch_page(
    backend: Backend,
    tab: SearchTab,
    keyword: String,
    page: u32,
    order: SearchOrder,
) -> Result<Page<SearchItem, u32>, ApiError> {
    match tab {
        SearchTab::All | SearchTab::Video => {
            let result = backend.search_videos(&keyword, page, order).await?;
            Ok(page_of(
                result
                    .videos
                    .into_iter()
                    .map(|video| SearchItem::Video(VideoCard::from(video))),
                page,
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
            Ok(page_of(
                result
                    .items
                    .into_iter()
                    .map(|item| SearchItem::Media(item.into())),
                page,
                result.num_pages,
            ))
        }
        SearchTab::Live => {
            let result = backend.search_live(&keyword, page, order).await?;
            Ok(page_of(
                result
                    .rooms
                    .into_iter()
                    .map(|room| SearchItem::Live(room.into())),
                page,
                result.num_pages,
            ))
        }
        SearchTab::Article => {
            let result = backend.search_articles(&keyword, page, order).await?;
            Ok(page_of(
                result
                    .articles
                    .into_iter()
                    .map(|article| SearchItem::Article(article.into())),
                page,
                result.num_pages,
            ))
        }
        SearchTab::User => {
            let result = backend.search_users(&keyword, page, order).await?;
            Ok(page_of(
                result
                    .users
                    .into_iter()
                    .map(|user| SearchItem::User(user.into())),
                page,
                result.num_pages,
            ))
        }
    }
}

fn page_of(
    items: impl IntoIterator<Item = SearchItem>,
    page: u32,
    num_pages: u32,
) -> Page<SearchItem, u32> {
    Page::new(items, has_more(page, num_pages), page)
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
