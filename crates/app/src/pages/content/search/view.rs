rigili_macro::mod_prv!(cards, feed, items, list, toolbar, user);

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use gpui_kit::component::alert::Alert;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::search::{SearchOrder, SearchType};
use rigili_bilibili::{Client, SpaceRelationStatus};

use items::SearchItem;
use rigili_core::client;
use rigili_core::login as login_state;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;
use rigili_core::{
    OpenLive, OpenSpace, OpenVideo, Refresh, api_error_message, api_task_error_message,
};

use crate::pages::account::login;
use crate::shared::browse::open::OpenTargets;
use crate::shared::browse::paging::Paged;
use crate::shared::browse::selection::{CardCursor, CardNav};
use crate::shared::ui::tabs::TabNav;

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
    list: Paged<SearchItem, u32>,
    users: Vec<items::UserItem>,
}

#[derive(Clone, Copy, Default)]
struct FollowState {
    followed: bool,
    status_loading: bool,
    toggling: bool,
}

impl TabState {
    fn new() -> Self {
        Self {
            list: Paged::new(),
            users: Vec::new(),
        }
    }
}

impl Deref for TabState {
    type Target = Paged<SearchItem, u32>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for TabState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

pub struct SearchView {
    client: Option<Arc<Client>>,
    keyword: String,
    tab: SearchTab,
    order: SearchOrder,
    tabs: [TabState; 7],
    counts: BTreeMap<String, u64>,
    follows: BTreeMap<u64, FollowState>,
    opening: Option<u64>,
    notice: Option<String>,
    list_width: Pixels,
    cards: CardCursor,
}

impl EventEmitter<OpenVideo> for SearchView {}
impl EventEmitter<OpenLive> for SearchView {}
impl EventEmitter<OpenSpace> for SearchView {}
impl OpenTargets for SearchView {}

impl crate::shared::browse::list::ListWidth for SearchView {
    fn list_width(&self) -> Pixels {
        self.list_width
    }

    fn set_list_width(&mut self, width: Pixels) {
        self.list_width = width;
    }
}

impl crate::shared::browse::season::SeasonOpener for SearchView {
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

impl SearchView {
    pub fn new(card_focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cards = CardCursor::new(card_focus);
        cards.observe(window, cx);
        let (client, error) = match client::get(cx) {
            Ok(client) => (Some(client), None),
            Err(error) => (None, Some(api_error_message(error))),
        };
        let mut view = Self {
            client,
            keyword: String::new(),
            tab: SearchTab::default(),
            order: SearchOrder::Default,
            tabs: std::array::from_fn(|_| TabState::new()),
            counts: BTreeMap::new(),
            follows: BTreeMap::new(),
            opening: None,
            notice: None,
            list_width: px(0.),
            cards,
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
        self.follows.clear();
        self.opening = None;
        self.notice = None;
        self.cards.clear();
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
        self.follows.clear();
        self.notice = None;
        self.cards.clear();

        if self.keyword.is_empty() {
            cx.notify();
            return;
        }

        let tab = self.tab;
        self.load_tab(tab, cx);
    }

    fn show_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }

        self.tab = tab;
        self.notice = None;
        self.cards.clear();
        let needs_load = self.state(tab).needs_first_load();
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
        self.follows.clear();
        let tab = self.tab;
        self.load_tab(tab, cx);
    }

    fn user_content_width(&self) -> Pixels {
        px((self.list_width.as_f32() - 32.).max(0.))
    }

    fn header_offset(&self) -> usize {
        usize::from(self.tab == SearchTab::All && !self.state(self.tab).users.is_empty())
    }

    fn sync_follows(&mut self, mids: Vec<u64>, cx: &mut Context<Self>) {
        if mids.is_empty() || login_state::user(cx).is_none() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let mids = mids
            .into_iter()
            .filter(|mid| !self.follows.contains_key(mid))
            .collect::<Vec<_>>();
        if mids.is_empty() {
            return;
        }

        for &mid in &mids {
            self.follows.insert(
                mid,
                FollowState {
                    status_loading: true,
                    ..Default::default()
                },
            );
        }
        cx.notify();

        let requested = mids.clone();
        spawn_api(
            cx,
            async move { client.relation_statuses(&mids).await },
            move |this, result, cx| {
                match result {
                    Ok(statuses) => {
                        for mid in requested {
                            if let Some(state) = this.follows.get_mut(&mid)
                                && state.status_loading
                            {
                                state.followed = statuses
                                    .get(&mid)
                                    .is_some_and(SpaceRelationStatus::is_followed);
                                state.status_loading = false;
                            }
                        }
                    }
                    Err(_) => {
                        for mid in requested {
                            if let Some(state) = this.follows.get_mut(&mid)
                                && state.status_loading
                            {
                                state.status_loading = false;
                            }
                        }
                    }
                }
                cx.notify();
            },
        );
    }

    fn toggle_follow(&mut self, mid: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if login_state::user(cx).is_none() {
            login::open(window, cx);
            return;
        }
        let follow = {
            let state = self.follows.entry(mid).or_default();
            if state.toggling || state.status_loading {
                return;
            }
            state.toggling = true;
            !state.followed
        };
        let handle = window.window_handle();
        cx.notify();

        spawn_api(
            cx,
            async move { client.modify_relation(mid, follow).await },
            move |this, result, cx| match result {
                Ok(()) => {
                    if let Some(state) = this.follows.get_mut(&mid) {
                        state.toggling = false;
                        state.followed = follow;
                    }
                    let note = Notification::success(if follow {
                        "已关注"
                    } else {
                        "已取消关注"
                    });
                    let _ = handle.update(cx, |_, window, cx| {
                        toast::push(window, cx, note);
                    });
                    cx.notify();
                }
                Err(error) => {
                    if let Some(state) = this.follows.get_mut(&mid) {
                        state.toggling = false;
                    }
                    let note = Notification::error(api_task_error_message(error, cx));
                    let _ = handle.update(cx, |_, window, cx| {
                        toast::push(window, cx, note);
                    });
                    cx.notify();
                }
            },
        );
    }
}

impl TabNav for SearchView {
    type Tab = SearchTab;

    const TABS: &'static [SearchTab] = &SearchTab::ALL;

    fn active_tab(&self) -> SearchTab {
        self.tab
    }

    fn select_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        self.show_tab(tab, cx);
    }
}

impl CardNav for SearchView {
    fn cards(&self) -> &CardCursor {
        &self.cards
    }

    fn cards_mut(&mut self) -> &mut CardCursor {
        &mut self.cards
    }

    fn card_count(&self) -> usize {
        self.state(self.tab).items.len()
    }

    fn card_columns(&self) -> usize {
        match self.tab.layout() {
            SearchLayout::Grid => self.columns(),
            SearchLayout::Rows => 1,
        }
    }

    fn card_start(&self) -> usize {
        first_card(self.cards.first_row(), self.header_offset(), self.card_columns())
    }

    fn reveal_card(&self, index: usize) {
        let row = card_row(index, self.header_offset(), self.card_columns());
        self.state(self.tab)
            .scroll
            .scroll_to_item(row, ScrollStrategy::Nearest);
    }

    fn open_card(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(index) = self.cards.index() else {
            return;
        };
        let Some(target) = self.state(self.tab).items.get(index).map(SearchItem::target) else {
            return;
        };
        self.open_target(target, new_tab, cx);
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

fn first_card(first_row: usize, header: usize, columns: usize) -> usize {
    first_row.saturating_sub(header) * columns.max(1)
}

fn card_row(index: usize, header: usize, columns: usize) -> usize {
    index / columns.max(1) + header
}

#[cfg(test)]
mod tests {
    use super::{SearchTab, card_row, first_card};

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

    #[test]
    fn accounts_for_the_user_block_above_the_results() {
        assert_eq!(first_card(0, 0, 3), 0);
        assert_eq!(first_card(4, 0, 3), 12);
        assert_eq!(first_card(0, 1, 3), 0);
        assert_eq!(first_card(4, 1, 3), 9);

        assert_eq!(card_row(0, 0, 3), 0);
        assert_eq!(card_row(7, 0, 3), 2);
        assert_eq!(card_row(0, 1, 3), 1);
        assert_eq!(card_row(7, 1, 3), 3);
    }
}
