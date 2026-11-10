use gpui_kit::*;
use rigili_bilibili::{LiveRoom, PgcIndex, Popular, Recommend};

use super::{HomeItem, HomeTab, HomeView, OpenLive, OpenTarget};
use crate::backend;
use crate::format::api_task_error_message;
use crate::grid::{VideoCard, append_unique_by};
use crate::platform::runtime::{ApiTaskError, spawn_api};

const VIDEO_PAGE_SIZE: u32 = 30;
const LIVE_PAGE_SIZE: u32 = 30;
const PGC_PAGE_SIZE: u32 = 20;

struct Page {
    items: Vec<HomeItem>,
    has_more: bool,
}

impl HomeView {
    pub(super) fn load_tab(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        if self.backend.is_none() {
            match backend::get(cx) {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    let state = self.state_mut(tab);
                    state.loading = false;
                    state.error = Some(error);
                    cx.notify();
                    return;
                }
            }
        }

        let Some(backend) = self.backend.clone() else {
            return;
        };
        let generation = {
            let state = self.state_mut(tab);
            state.generation += 1;
            state.loading = true;
            state.loading_more = false;
            state.error = None;
            state.more_error = None;
            state.items.clear();
            state.page = 0;
            state.has_more = false;
            state.generation
        };
        cx.notify();

        match tab {
            HomeTab::Recommend => spawn_api(
                cx,
                async move {
                    backend
                        .recommend(1, VIDEO_PAGE_SIZE)
                        .await
                        .map(recommend_page)
                },
                move |this, result, cx| this.finish_first_page(tab, generation, result, cx),
            ),
            HomeTab::Popular => spawn_api(
                cx,
                async move { backend.popular(1, VIDEO_PAGE_SIZE).await.map(popular_page) },
                move |this, result, cx| this.finish_first_page(tab, generation, result, cx),
            ),
            HomeTab::Live => spawn_api(
                cx,
                async move { backend.live_list(1, LIVE_PAGE_SIZE).await.map(live_page) },
                move |this, result, cx| this.finish_first_page(tab, generation, result, cx),
            ),
            HomeTab::Bangumi => spawn_api(
                cx,
                async move { backend.pgc_index(1, 1, PGC_PAGE_SIZE).await.map(pgc_page) },
                move |this, result, cx| this.finish_first_page(tab, generation, result, cx),
            ),
            HomeTab::Cinema => spawn_api(
                cx,
                async move { backend.pgc_index(2, 1, PGC_PAGE_SIZE).await.map(pgc_page) },
                move |this, result, cx| this.finish_first_page(tab, generation, result, cx),
            ),
        }
    }

    fn finish_first_page(
        &mut self,
        tab: HomeTab,
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
                state.page = 1;
                state.has_more = page.has_more;
                state.items = page.items;
                state.scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            Err(error) => state.error = Some(api_task_error_message(error, cx)),
        }
        cx.notify();
    }

    pub(super) fn load_more(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        let (page, generation) = {
            let state = self.state(tab);
            if state.loading || state.loading_more || !state.has_more || state.more_error.is_some()
            {
                return;
            }
            (state.page + 1, state.generation)
        };
        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.state_mut(tab).loading_more = true;
        cx.notify();

        match tab {
            HomeTab::Recommend => spawn_api(
                cx,
                async move {
                    backend
                        .recommend(page, VIDEO_PAGE_SIZE)
                        .await
                        .map(recommend_page)
                },
                move |this, result, cx| this.finish_more(tab, page, generation, result, cx),
            ),
            HomeTab::Popular => spawn_api(
                cx,
                async move {
                    backend
                        .popular(page, VIDEO_PAGE_SIZE)
                        .await
                        .map(popular_page)
                },
                move |this, result, cx| this.finish_more(tab, page, generation, result, cx),
            ),
            HomeTab::Live => spawn_api(
                cx,
                async move { backend.live_list(page, LIVE_PAGE_SIZE).await.map(live_page) },
                move |this, result, cx| this.finish_more(tab, page, generation, result, cx),
            ),
            HomeTab::Bangumi => spawn_api(
                cx,
                async move {
                    backend
                        .pgc_index(1, page, PGC_PAGE_SIZE)
                        .await
                        .map(pgc_page)
                },
                move |this, result, cx| this.finish_more(tab, page, generation, result, cx),
            ),
            HomeTab::Cinema => spawn_api(
                cx,
                async move {
                    backend
                        .pgc_index(2, page, PGC_PAGE_SIZE)
                        .await
                        .map(pgc_page)
                },
                move |this, result, cx| this.finish_more(tab, page, generation, result, cx),
            ),
        }
    }

    fn finish_more(
        &mut self,
        tab: HomeTab,
        page_number: u32,
        generation: u64,
        result: Result<Page, ApiTaskError>,
        cx: &mut Context<Self>,
    ) {
        if self.state(tab).generation != generation {
            return;
        }

        let state = self.state_mut(tab);
        state.loading_more = false;
        match result {
            Ok(page) => {
                state.page = page_number;
                state.has_more = page.has_more;
                append_unique_by(&mut state.items, page.items, HomeItem::key);
            }
            Err(error) => state.more_error = Some(api_task_error_message(error, cx)),
        }
        cx.notify();
    }

    pub(super) fn retry_more(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        self.state_mut(tab).more_error = None;
        self.load_more(tab, cx);
    }

    pub(super) fn maybe_load_more(
        &mut self,
        tab: HomeTab,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows > 0 && last_visible + 1 >= rows {
            self.load_more(tab, cx);
        }
    }

    pub(super) fn open_target(&mut self, target: OpenTarget, cx: &mut Context<Self>) {
        match target {
            OpenTarget::Video(video) => cx.emit(video),
            OpenTarget::Live { room_id, title } => cx.emit(OpenLive { room_id, title }),
            OpenTarget::Season(season_id) => self.open_season(season_id, cx),
        }
    }

    fn open_season(&mut self, season_id: u64, cx: &mut Context<Self>) {
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
            async move { crate::playback::resolve_season(backend, season_id).await },
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

fn recommend_page(recommend: Recommend) -> Page {
    let has_more = recommend.item.len() >= VIDEO_PAGE_SIZE as usize;
    let items = recommend
        .item
        .into_iter()
        .filter(|item| !item.bvid.is_empty() && item.cid != 0)
        .map(|item| HomeItem::Video(VideoCard::from(item)))
        .collect();
    Page { items, has_more }
}

fn popular_page(popular: Popular) -> Page {
    Page {
        has_more: has_more(&popular),
        items: popular
            .list
            .into_iter()
            .map(|video| HomeItem::Video(VideoCard::from(video)))
            .collect(),
    }
}

fn live_page(rooms: Vec<LiveRoom>) -> Page {
    Page {
        has_more: rooms.len() >= LIVE_PAGE_SIZE as usize,
        items: rooms.into_iter().map(HomeItem::from).collect(),
    }
}

fn pgc_page(index: PgcIndex) -> Page {
    Page {
        has_more: index.has_next != 0,
        items: index.list.into_iter().map(HomeItem::from).collect(),
    }
}

fn has_more(popular: &Popular) -> bool {
    !popular.no_more && !popular.list.is_empty()
}

#[cfg(test)]
mod tests {
    use super::{HomeItem, has_more, pgc_page, recommend_page};
    use rigili_bilibili::{PgcIndex, PgcItem, Popular, PopularVideo, Recommend, RecommendVideo};

    #[test]
    fn stops_when_the_feed_is_empty_or_exhausted() {
        let empty = Popular {
            list: Vec::new(),
            no_more: false,
        };
        assert!(!has_more(&empty));

        let last = Popular {
            list: vec![popular_video("BV1")],
            no_more: true,
        };
        assert!(!has_more(&last));

        let more = Popular {
            list: vec![popular_video("BV1")],
            no_more: false,
        };
        assert!(has_more(&more));
    }

    #[test]
    fn filters_recommend_cards_without_videos() {
        let page = recommend_page(Recommend {
            item: vec![
                RecommendVideo {
                    bvid: "BV1".into(),
                    cid: 1,
                    ..Default::default()
                },
                RecommendVideo {
                    title: "直播卡片".into(),
                    ..Default::default()
                },
            ],
        });

        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items.iter().map(HomeItem::key).collect::<Vec<_>>(),
            ["video:BV1"]
        );
        assert!(!page.has_more);
    }

    #[test]
    fn follows_pgc_has_next() {
        let page = pgc_page(PgcIndex {
            has_next: 1,
            list: vec![pgc_item(1)],
            total: 2,
        });
        assert!(page.has_more);
        assert_eq!(page.items.len(), 1);

        let page = pgc_page(PgcIndex {
            has_next: 0,
            list: vec![pgc_item(1)],
            total: 1,
        });
        assert!(!page.has_more);
    }

    fn popular_video(bvid: &str) -> PopularVideo {
        PopularVideo {
            aid: 0,
            bvid: bvid.into(),
            cid: 0,
            pic: String::new(),
            title: String::new(),
            duration: 0,
            pubdate: 0,
            owner: rigili_bilibili::Owner::default(),
            stat: rigili_bilibili::Stat::default(),
        }
    }

    fn pgc_item(season_id: u64) -> PgcItem {
        PgcItem {
            season_id,
            ..Default::default()
        }
    }
}
