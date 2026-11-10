rigili_macro::mod_prv!(feed, list);

use gpui_kit::assets::IconName;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{LiveRoom, PgcItem};

use crate::backend::{self, Backend};
use crate::format::format_count;
use crate::grid::{OpenVideo, VideoCard};
use crate::refresh::Refresh;

#[derive(Clone)]
pub(crate) struct OpenLive {
    pub(crate) room_id: u64,
    pub(crate) title: String,
}

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

pub(crate) struct LiveCard {
    room_id: u64,
    cover: String,
    title: String,
    meta: String,
    online: String,
}

pub(crate) struct PgcCard {
    season_id: u64,
    cover: String,
    title: String,
    subtitle: String,
    score: String,
    index_show: String,
    badge: Option<String>,
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
            Self::Video(video) => format!("video:{}", video.bvid()),
            Self::Live(live) => format!("live:{}", live.room_id),
            Self::Pgc(pgc) => format!("pgc:{}", pgc.season_id),
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

    fn cover(&self) -> &str {
        match self {
            Self::Video(video) => video.cover(),
            Self::Live(live) => &live.cover,
            Self::Pgc(pgc) => &pgc.cover,
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Video(video) => video.title(),
            Self::Live(live) => &live.title,
            Self::Pgc(pgc) => &pgc.title,
        }
    }

    fn footer(&self) -> &str {
        match self {
            Self::Video(video) => video.meta(),
            Self::Live(live) => &live.meta,
            Self::Pgc(pgc) => &pgc.subtitle,
        }
    }

    fn leading(&self) -> Option<(IconName, &str)> {
        match self {
            Self::Video(video) => Some((IconName::Play, video.views())),
            Self::Live(live) => Some((IconName::Eye, &live.online)),
            Self::Pgc(pgc) => {
                (!pgc.score.is_empty()).then_some((IconName::Star, pgc.score.as_str()))
            }
        }
    }

    fn secondary(&self) -> Option<(Option<IconName>, &str)> {
        match self {
            Self::Video(video) => Some((Some(IconName::MessageCircle), video.danmaku())),
            Self::Live(_) => None,
            Self::Pgc(_) => None,
        }
    }

    fn trailing(&self) -> Option<&str> {
        match self {
            Self::Video(video) => (!video.duration().is_empty()).then_some(video.duration()),
            Self::Live(_) => Some("直播中"),
            Self::Pgc(pgc) => (!pgc.index_show.is_empty()).then_some(pgc.index_show.as_str()),
        }
    }

    fn badge(&self) -> Option<&str> {
        match self {
            Self::Video(_) | Self::Live(_) => None,
            Self::Pgc(pgc) => pgc.badge.as_deref(),
        }
    }
}

impl From<LiveRoom> for HomeItem {
    fn from(room: LiveRoom) -> Self {
        let cover = if room.cover.is_empty() {
            room.system_cover
        } else {
            room.cover
        };
        let cover = match cover.strip_prefix("//") {
            Some(rest) => format!("https://{rest}"),
            None => cover,
        };
        let meta = if room.area_name.is_empty() {
            room.uname
        } else {
            format!("{} · {}", room.uname, room.area_name)
        };

        Self::Live(LiveCard {
            room_id: room.roomid,
            cover,
            title: room.title,
            meta,
            online: format_count(room.online),
        })
    }
}

impl From<PgcItem> for HomeItem {
    fn from(item: PgcItem) -> Self {
        Self::Pgc(PgcCard {
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: (!item.badge.is_empty()).then_some(item.badge),
        })
    }
}

pub(crate) struct TabState {
    items: Vec<HomeItem>,
    page: u32,
    has_more: bool,
    loading: bool,
    loading_more: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
    scroll: VirtualListScrollHandle,
}

impl TabState {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            page: 0,
            has_more: false,
            loading: false,
            loading_more: false,
            error: None,
            more_error: None,
            generation: 0,
            scroll: VirtualListScrollHandle::new(),
        }
    }
}

pub struct HomeView {
    backend: Option<Backend>,
    tab: HomeTab,
    tabs: [TabState; 5],
    opening: Option<u64>,
    notice: Option<String>,
    list_width: Pixels,
}

impl EventEmitter<OpenVideo> for HomeView {}
impl EventEmitter<OpenLive> for HomeView {}

impl HomeView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (backend, error) = match backend::get(cx) {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error)),
        };
        let mut view = Self {
            backend,
            tab: HomeTab::default(),
            tabs: std::array::from_fn(|_| TabState::new()),
            opening: None,
            notice: None,
            list_width: px(0.),
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
    use super::{HomeItem, HomeTab, OpenTarget, PgcCard};
    use gpui_kit::assets::IconName;
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
        assert_eq!(item.trailing(), Some("全8话"));
        assert_eq!(item.badge(), Some("大会员"));
        assert!(matches!(item.target(), OpenTarget::Season(47836)));
        assert_eq!(
            item.leading().map(|(icon, text)| (icon, text.to_owned())),
            Some((IconName::Star, "9.7".to_owned()))
        );
    }

    #[test]
    fn leaves_empty_subtitles_and_badges_out_of_cards() {
        let card = PgcCard {
            season_id: 1,
            cover: String::new(),
            title: "作品".into(),
            subtitle: String::new(),
            score: String::new(),
            index_show: String::new(),
            badge: None,
        };
        let item = HomeItem::Pgc(card);
        assert_eq!(item.footer(), "");
        assert_eq!(item.leading(), None);
        assert_eq!(item.trailing(), None);
        assert_eq!(item.badge(), None);
    }
}
