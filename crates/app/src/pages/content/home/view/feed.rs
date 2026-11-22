use std::sync::Arc;

use gpui_kit::*;
use rigili_bilibili::{ApiError, Client, LiveRoom, PgcIndex, Popular, Recommend};

use super::{HomeItem, HomeTab, HomeView};
use rigili_core::client;

use crate::shared::browse::grid::VideoCard;
use crate::shared::browse::paging::{Page, spawn_first, spawn_more};

const VIDEO_PAGE_SIZE: u32 = 30;
const LIVE_PAGE_SIZE: u32 = 30;
const PGC_PAGE_SIZE: u32 = 20;

impl HomeView {
    fn cached_client(&mut self, cx: &mut Context<Self>) -> Result<Arc<Client>, String> {
        client::cached(&mut self.client, cx)
    }

    pub(super) fn load_tab(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.state_mut(tab).fail_first(error);
                cx.notify();
                return;
            }
        };

        spawn_first(
            self,
            cx,
            move |this| this.state_mut(tab),
            HomeItem::key,
            async move { fetch_page(&client, tab, 1).await },
            move |this| {
                this.cards.clear();
                this.state_mut(tab)
                    .scroll
                    .scroll_to_item(0, ScrollStrategy::Top);
            },
        );
    }

    pub(super) fn load_more(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };

        spawn_more(
            self,
            cx,
            move |this| this.state_mut(tab),
            HomeItem::key,
            move |page| {
                let page = page + 1;
                async move { fetch_page(&client, tab, page).await }
            },
            |_| {},
        );
    }

    pub(super) fn retry_more(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        self.state_mut(tab).retry_more();
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
}

impl crate::shared::browse::season::SeasonOpener for HomeView {
    fn opening(&mut self) -> &mut Option<u64> {
        &mut self.opening
    }

    fn notice(&mut self) -> &mut Option<String> {
        &mut self.notice
    }

    fn season_client(&self) -> Option<Arc<Client>> {
        self.client.clone()
    }
}

async fn fetch_page(
    client: &Client,
    tab: HomeTab,
    page: u32,
) -> Result<Page<HomeItem, u32>, ApiError> {
    match tab {
        HomeTab::Recommend => client
            .recommend(page, VIDEO_PAGE_SIZE)
            .await
            .map(|result| recommend_page(result, page)),
        HomeTab::Popular => client
            .popular(page, VIDEO_PAGE_SIZE)
            .await
            .map(|result| popular_page(result, page)),
        HomeTab::Live => client
            .live_list(page, LIVE_PAGE_SIZE)
            .await
            .map(|result| live_page(result, page)),
        HomeTab::Bangumi => client
            .pgc_index(1, page, PGC_PAGE_SIZE)
            .await
            .map(|result| pgc_page(result, page)),
        HomeTab::Cinema => client
            .pgc_index(2, page, PGC_PAGE_SIZE)
            .await
            .map(|result| pgc_page(result, page)),
    }
}

fn recommend_page(recommend: Recommend, page: u32) -> Page<HomeItem, u32> {
    let has_more = recommend.item.len() >= VIDEO_PAGE_SIZE as usize;
    let items: Vec<HomeItem> = recommend
        .item
        .into_iter()
        .filter(|item| !item.bvid.is_empty() && item.cid != 0)
        .map(|item| HomeItem::Video(VideoCard::from(item)))
        .collect();
    Page::new(items, has_more, page)
}

fn popular_page(popular: Popular, page: u32) -> Page<HomeItem, u32> {
    let has_more = has_more(&popular);
    Page::new(
        popular
            .list
            .into_iter()
            .map(|video| HomeItem::Video(VideoCard::from(video))),
        has_more,
        page,
    )
}

fn live_page(rooms: Vec<LiveRoom>, page: u32) -> Page<HomeItem, u32> {
    let has_more = rooms.len() >= LIVE_PAGE_SIZE as usize;
    Page::new(rooms.into_iter().map(HomeItem::from), has_more, page)
}

fn pgc_page(index: PgcIndex, page: u32) -> Page<HomeItem, u32> {
    Page::new(
        index.list.into_iter().map(HomeItem::from),
        index.has_next != 0,
        page,
    )
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
        let page = recommend_page(
            Recommend {
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
            },
            1,
        );

        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items.iter().map(HomeItem::key).collect::<Vec<_>>(),
            ["video:BV1"]
        );
        assert!(!page.has_more);
        assert_eq!(page.meta, 1);
    }

    #[test]
    fn follows_pgc_has_next() {
        let page = pgc_page(
            PgcIndex {
                has_next: 1,
                list: vec![pgc_item(1)],
                total: 2,
            },
            1,
        );
        assert!(page.has_more);
        assert_eq!(page.items.len(), 1);

        let page = pgc_page(
            PgcIndex {
                has_next: 0,
                list: vec![pgc_item(1)],
                total: 1,
            },
            1,
        );
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
