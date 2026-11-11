rigili_macro::mod_prv!(cards, feed, items, list, toolbar, user);

use std::collections::{BTreeMap, HashSet};

use gpui_kit::component::alert::Alert;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::search::{SearchOrder, SearchType};

use items::SearchItem;
use rigili_core::backend::{self, Backend};
use rigili_core::{OpenLive, OpenSpace, OpenVideo, Refresh, api_error_message};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchTab {
    #[default]
    All,
    Video,
    Bangumi,
    Film,
    Live,
    Article,
    User,
}

impl SearchTab {
    pub(crate) const ALL: [Self; 7] = [
        Self::All,
        Self::Video,
        Self::Bangumi,
        Self::Film,
        Self::Live,
        Self::Article,
        Self::User,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "综合",
            Self::Video => "视频",
            Self::Bangumi => "番剧",
            Self::Film => "影视",
            Self::Live => "直播",
            Self::Article => "专栏",
            Self::User => "用户",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::Video => 1,
            Self::Bangumi => 2,
            Self::Film => 3,
            Self::Live => 4,
            Self::Article => 5,
            Self::User => 6,
        }
    }

    fn search_type(self) -> Option<SearchType> {
        match self {
            Self::All => None,
            Self::Video => Some(SearchType::Video),
            Self::Bangumi => Some(SearchType::Bangumi),
            Self::Film => Some(SearchType::Film),
            Self::Live => Some(SearchType::Live),
            Self::Article => Some(SearchType::Article),
            Self::User => Some(SearchType::User),
        }
    }

    fn count_key(self) -> Option<&'static str> {
        self.search_type().map(SearchType::as_str)
    }

    fn supports_order(self) -> bool {
        matches!(self, Self::All | Self::Video | Self::Article)
    }

    fn empty_label(self) -> &'static str {
        match self {
            Self::All => "内容",
            _ => self.label(),
        }
    }

    fn layout(self) -> SearchLayout {
        match self {
            Self::Article | Self::User => SearchLayout::Rows,
            _ => SearchLayout::Grid,
        }
    }

    fn list_id(self) -> &'static str {
        match self {
            Self::All => "search-all-list",
            Self::Video => "search-video-list",
            Self::Bangumi => "search-bangumi-list",
            Self::Film => "search-film-list",
            Self::Live => "search-live-list",
            Self::Article => "search-article-list",
            Self::User => "search-user-list",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchLayout {
    Grid,
    Rows,
}

struct TabState {
    items: Vec<SearchItem>,
    seen: HashSet<String>,
    users: Vec<items::UserItem>,
    page: u32,
    num_pages: u32,
    generation: u64,
    loading: bool,
    loading_more: bool,
    error: Option<String>,
    more_error: Option<String>,
    loaded: bool,
    scroll: VirtualListScrollHandle,
}

impl TabState {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            seen: HashSet::new(),
            users: Vec::new(),
            page: 0,
            num_pages: 0,
            generation: 0,
            loading: false,
            loading_more: false,
            error: None,
            more_error: None,
            loaded: false,
            scroll: VirtualListScrollHandle::new(),
        }
    }
}

pub struct SearchView {
    backend: Option<Backend>,
    keyword: String,
    tab: SearchTab,
    order: SearchOrder,
    tabs: [TabState; 7],
    counts: BTreeMap<String, u64>,
    opening: Option<u64>,
    notice: Option<String>,
    list_width: Pixels,
}

impl EventEmitter<OpenVideo> for SearchView {}
impl EventEmitter<OpenLive> for SearchView {}
impl EventEmitter<OpenSpace> for SearchView {}

impl SearchView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (backend, error) = match backend::get(cx) {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(api_error_message(error))),
        };
        let mut view = Self {
            backend,
            keyword: String::new(),
            tab: SearchTab::default(),
            order: SearchOrder::Default,
            tabs: std::array::from_fn(|_| TabState::new()),
            counts: BTreeMap::new(),
            opening: None,
            notice: None,
            list_width: px(0.),
        };
        if let Some(error) = error {
            view.state_mut(SearchTab::default()).error = Some(error);
        }
        view
    }

    fn state(&self, tab: SearchTab) -> &TabState {
        &self.tabs[tab.index()]
    }

    fn state_mut(&mut self, tab: SearchTab) -> &mut TabState {
        &mut self.tabs[tab.index()]
    }

    pub(crate) fn search(&mut self, keyword: String, cx: &mut Context<Self>) {
        self.keyword = keyword.trim().to_owned();
        self.tab = SearchTab::default();
        self.counts.clear();
        self.opening = None;
        self.notice = None;
        self.tabs = std::array::from_fn(|_| TabState::new());

        if self.keyword.is_empty() {
            cx.notify();
            return;
        }

        self.load_tab(SearchTab::All, cx);
    }

    pub(crate) fn set_order(&mut self, order: SearchOrder, cx: &mut Context<Self>) {
        if self.order == order {
            return;
        }

        self.order = order;
        self.tabs = std::array::from_fn(|_| TabState::new());
        self.notice = None;

        if self.keyword.is_empty() {
            cx.notify();
            return;
        }

        let tab = self.tab;
        self.load_tab(tab, cx);
    }

    pub(crate) fn show_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }

        self.tab = tab;
        self.notice = None;
        let needs_load = {
            let state = self.state(tab);
            !state.loaded && !state.loading && state.error.is_none()
        };
        if needs_load {
            self.load_tab(tab, cx);
        } else {
            cx.notify();
        }
    }

    fn count(&self, tab: SearchTab) -> Option<u64> {
        tab.count_key()
            .and_then(|key| self.counts.get(key).copied())
    }

    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        if self.keyword.is_empty() {
            return;
        }
        self.notice = None;
        let tab = self.tab;
        self.load_tab(tab, cx);
    }

    fn user_content_width(&self) -> Pixels {
        px((self.list_width.as_f32() - 32.).max(0.))
    }
}

impl Refresh for SearchView {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn can_refresh(&self) -> bool {
        !self.keyword.is_empty()
    }
}

impl Render for SearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let content = self.render_content(window, cx);
        let notice = self.notice.clone();
        let orders = self.tab.supports_order().then(|| self.render_orders(cx));

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_4()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .when_some(notice, |this, notice| {
                this.child(
                    Alert::error("search-notice", notice)
                        .title("打开失败")
                        .on_close(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                )
            })
            .child(self.render_tabs(cx))
            .when_some(orders, |this, orders| this.child(orders))
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::SearchTab;

    #[test]
    fn orders_search_tabs_like_the_reference_layout() {
        assert_eq!(
            SearchTab::ALL.map(SearchTab::label),
            ["综合", "视频", "番剧", "影视", "直播", "专栏", "用户"]
        );
        assert_eq!(SearchTab::default(), SearchTab::All);
        for (index, tab) in SearchTab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), index);
        }
        assert!(SearchTab::All.supports_order());
        assert!(SearchTab::Video.supports_order());
        assert!(!SearchTab::Bangumi.supports_order());
        assert!(!SearchTab::User.supports_order());
    }
}
