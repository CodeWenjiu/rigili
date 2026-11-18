rigili_macro::mod_prv!(feed, list);

use std::sync::Arc;

use crate::shared::browse::paging::Paged;

use gpui_kit::component::alert::Alert;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Client, LiveRoom, PgcItem};

use crate::shared::browse::grid::{LiveCard, PgcCard, VideoCard};
use rigili_core::client;
use rigili_core::{OpenLive, OpenSpace, OpenVideo, Refresh, api_error_message};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HomeTab {
    Live,
    #[default]
    Recommend,
    Popular,
    Bangumi,
    Cinema,
}

impl HomeTab {
    pub(crate) const ALL: [Self; 5] = [
        Self::Live,
        Self::Recommend,
        Self::Popular,
        Self::Bangumi,
        Self::Cinema,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Live => "直播",
            Self::Recommend => "推荐",
            Self::Popular => "热门",
            Self::Bangumi => "追番",
            Self::Cinema => "影视",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Live => 0,
            Self::Recommend => 1,
            Self::Popular => 2,
            Self::Bangumi => 3,
            Self::Cinema => 4,
        }
    }

    fn list_id(self) -> &'static str {
        match self {
            Self::Live => "home-live-list",
            Self::Recommend => "home-recommend-list",
            Self::Popular => "home-popular-list",
            Self::Bangumi => "home-bangumi-list",
            Self::Cinema => "home-cinema-list",
        }
    }
}

pub(crate) enum HomeItem {
    Video(VideoCard),
    Live(LiveCard),
    Pgc(PgcCard),
}

#[derive(Clone)]
pub(crate) enum OpenTarget {
    Video(OpenVideo),
    Live { room_id: u64, title: String },
    Season(u64),
}

impl HomeItem {
    pub(crate) fn key(&self) -> String {
        match self {
            Self::Video(video) => video.key(),
            Self::Live(live) => live.key(),
            Self::Pgc(pgc) => pgc.key(),
        }
    }

    pub(crate) fn target(&self) -> OpenTarget {
        match self {
            Self::Video(video) => OpenTarget::Video(video.open()),
            Self::Live(live) => OpenTarget::Live {
                room_id: live.room_id,
                title: live.title.clone(),
            },
            Self::Pgc(pgc) => OpenTarget::Season(pgc.season_id),
        }
    }

    fn season_id(&self) -> Option<u64> {
        match self {
            Self::Pgc(pgc) => Some(pgc.season_id),
            _ => None,
        }
    }

    pub(crate) fn cover(&self) -> Option<&str> {
        match self {
            Self::Video(video) => Some(video.cover()),
            Self::Live(live) => Some(live.cover.as_str()),
            Self::Pgc(pgc) => Some(pgc.cover.as_str()),
        }
    }
}

impl From<LiveRoom> for HomeItem {
    fn from(room: LiveRoom) -> Self {
        Self::Live(LiveCard::from(room))
    }
}

impl From<PgcItem> for HomeItem {
    fn from(item: PgcItem) -> Self {
        Self::Pgc(PgcCard::from(item))
    }
}

pub(crate) type TabState = Paged<HomeItem, u32>;

pub struct HomeView {
    client: Option<Arc<Client>>,
    tab: HomeTab,
    tabs: [TabState; 5],
    opening: Option<u64>,
    notice: Option<String>,
    list_width: Pixels,
    hovered: Option<SharedString>,
    prefetched: Option<(HomeTab, usize, usize)>,
}

impl EventEmitter<OpenVideo> for HomeView {}
impl EventEmitter<OpenLive> for HomeView {}
impl EventEmitter<OpenSpace> for HomeView {}

impl crate::shared::browse::list::ListWidth for HomeView {
    fn list_width(&self) -> Pixels {
        self.list_width
    }

    fn set_list_width(&mut self, width: Pixels) {
        self.list_width = width;
    }
}

impl HomeView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (client, error) = match client::get(cx) {
            Ok(client) => (Some(client), None),
            Err(error) => (None, Some(api_error_message(error))),
        };
        let mut view = Self {
            client,
            tab: HomeTab::default(),
            tabs: std::array::from_fn(|_| TabState::new()),
            opening: None,
            notice: None,
            list_width: px(0.),
            hovered: None,
            prefetched: None,
        };
        match error {
            Some(error) => {
                for tab in HomeTab::ALL {
                    view.state_mut(tab).error = Some(error.clone());
                }
            }
            None => view.load_tab(HomeTab::Recommend, cx),
        }
        view
    }

    pub(crate) fn set_hovered(&mut self, id: Option<SharedString>, cx: &mut Context<Self>) {
        if self.hovered == id {
            return;
        }
        self.hovered = id;
        cx.notify();
    }

    pub(crate) fn state(&self, tab: HomeTab) -> &TabState {
        &self.tabs[tab.index()]
    }

    pub(crate) fn state_mut(&mut self, tab: HomeTab) -> &mut TabState {
        &mut self.tabs[tab.index()]
    }

    pub(crate) fn current_tab(&self) -> HomeTab {
        self.tab
    }

    pub(crate) fn show_tab(&mut self, tab: HomeTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }

        self.tab = tab;
        self.notice = None;
        let needs_load = {
            let state = self.state(tab);
            state.items.is_empty() && !state.loading && state.error.is_none()
        };
        if needs_load {
            self.load_tab(tab, cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn reload_current(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        self.load_tab(tab, cx);
    }
}

impl Refresh for HomeView {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload_current(cx);
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let content = self.render_content(window, cx);
        let notice = self.notice.clone();

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .when_some(notice, |this, notice| {
                this.child(
                    Alert::error("home-notice", notice)
                        .title("打开失败")
                        .on_close(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                )
            })
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::{HomeItem, HomeTab, OpenTarget};
    use crate::shared::browse::grid::PgcCard;
    use rigili_bilibili::PgcItem;

    #[test]
    fn orders_tabs_like_the_reference_navigation() {
        assert_eq!(
            HomeTab::ALL.map(HomeTab::label),
            ["直播", "推荐", "热门", "追番", "影视"]
        );
        assert_eq!(HomeTab::default(), HomeTab::Recommend);
        for (index, tab) in HomeTab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), index);
        }
    }

    #[test]
    fn maps_pgc_cards_to_season_targets() {
        let item = HomeItem::from(PgcItem {
            season_id: 47836,
            title: "鬼灭之刃".into(),
            cover: String::new(),
            index_show: "全8话".into(),
            subtitle: "羁绊连结".into(),
            score: "9.7".into(),
            badge: "大会员".into(),
            season_type: 1,
        });
        let card = match &item {
            HomeItem::Pgc(card) => card,
            _ => panic!("expected pgc card"),
        };
        assert_eq!(card.season_id, 47836);
        assert_eq!(card.index_show, "全8话");
        assert_eq!(card.badge.as_deref(), Some("大会员"));
        assert_eq!(card.score, "9.7");
        assert!(matches!(item.target(), OpenTarget::Season(47836)));

        let card = PgcCard::from(PgcItem {
            season_id: 1,
            title: "作品".into(),
            subtitle: String::new(),
            score: String::new(),
            index_show: String::new(),
            badge: String::new(),
            ..Default::default()
        });
        assert!(card.subtitle.is_empty());
        assert_eq!(card.badge, None);
    }
}
