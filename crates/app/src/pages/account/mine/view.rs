rigili_macro::mod_prv!(
    create_folder,
    bangumi,
    favorites,
    feed,
    list,
    profile,
    sort_folders
);

use std::sync::Arc;

use gpui_kit::component::alert::Alert;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{BangumiFollow, Client, FavFolder, NavStat};
use rigili_core::client;
use rigili_core::{OpenSpace, OpenVideo, Refresh};
use serde::{Deserialize, Serialize};

use crate::app::preferences;
use crate::shared::browse::filter;
use crate::shared::browse::grid::{CardFrame, CardGrid, VideoCard, VideoCards};
use crate::shared::browse::list::ListScroll;
use crate::shared::browse::paging::Paged;
use crate::shared::browse::selection::{CardCursor, CardNav};
use crate::shared::ui::tabs::{TabNav, TabStrip};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MineTab {
    History,
    Offline,
    #[default]
    Favorites,
    WatchLater,
}

impl MineTab {
    pub(crate) const ALL: [Self; 4] = [
        Self::History,
        Self::Offline,
        Self::Favorites,
        Self::WatchLater,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::History => "历史记录",
            Self::Offline => "离线缓存",
            Self::Favorites => "我的收藏",
            Self::WatchLater => "稍后再看",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::History => 0,
            Self::Offline => 1,
            Self::Favorites => 2,
            Self::WatchLater => 3,
        }
    }

    fn list_id(self) -> &'static str {
        match self {
            Self::History => "mine-history-list",
            Self::Offline => "mine-offline-list",
            Self::Favorites => "mine-favorites-list",
            Self::WatchLater => "mine-watch-later-list",
        }
    }
}

#[derive(Clone, Copy)]
struct CardRow<'a> {
    filtered: Option<&'a [usize]>,
    row: usize,
    columns: usize,
    frame: CardFrame,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ListMeta {
    pub(crate) page: u32,
    pub(crate) max: u64,
    pub(crate) view_at: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FavoriteView {
    #[default]
    Folder,
    Folders,
    Subscribed,
    Bangumi,
    Cinema,
}

impl FavoriteView {
    const ALL: [Self; 5] = [
        Self::Folder,
        Self::Folders,
        Self::Subscribed,
        Self::Bangumi,
        Self::Cinema,
    ];

    fn index(self) -> usize {
        match self {
            Self::Folder => 0,
            Self::Folders => 1,
            Self::Subscribed => 2,
            Self::Bangumi => 3,
            Self::Cinema => 4,
        }
    }
}

struct FolderState {
    folders: Vec<FavFolder>,
    current: Option<u64>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
}

impl FolderState {
    fn new() -> Self {
        Self {
            folders: Vec::new(),
            current: None,
            loading: false,
            loaded: false,
            error: None,
            generation: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FolderSource {
    Created,
    Collected,
}

struct CollectedState {
    folders: Vec<FavFolder>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
}

impl CollectedState {
    fn new() -> Self {
        Self {
            folders: Vec::new(),
            loading: false,
            loaded: false,
            error: None,
            generation: 0,
        }
    }
}

pub struct MineView {
    client: Option<Arc<Client>>,
    tab: MineTab,
    folders: FolderState,
    favorites: Paged<VideoCard, ListMeta>,
    history: Paged<VideoCard, ListMeta>,
    watch_later: Paged<VideoCard, ListMeta>,
    collected: CollectedState,
    bangumi: Paged<BangumiFollow, u32>,
    cinema: Paged<BangumiFollow, u32>,
    stat: Option<NavStat>,
    stat_loading: bool,
    stat_error: Option<String>,
    favorites_view: FavoriteView,
    browsing_folder: Option<FavoriteView>,
    opening: Option<u64>,
    notice: Option<String>,
    user_mid: Option<u64>,
    list_width: Pixels,
    folder_scroll: ListScroll,
    favorites_keyword: String,
    search: filter::Search<MineView>,
    cards: CardCursor,
}

impl EventEmitter<OpenVideo> for MineView {}
impl EventEmitter<OpenSpace> for MineView {}

impl crate::shared::browse::list::ListWidth for MineView {
    fn list_width(&self) -> Pixels {
        self.list_width
    }

    fn set_list_width(&mut self, width: Pixels) {
        self.list_width = width;
    }
}

impl crate::shared::browse::season::SeasonOpener for MineView {
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

impl MineView {
    pub fn new(card_focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = client::get(cx).ok();
        let cards = CardCursor::new(card_focus);
        cards.observe(window, cx);
        let search = filter::Search::new(window, cx, "搜索收藏、历史与追番", |this, cx| {
            this.submit_search(cx)
        });
        Self {
            client,
            tab: preferences::mine_tab(cx),
            folders: FolderState::new(),
            favorites: Paged::new(),
            history: Paged::new(),
            watch_later: Paged::new(),
            collected: CollectedState::new(),
            bangumi: Paged::new(),
            cinema: Paged::new(),
            stat: None,
            stat_loading: false,
            stat_error: None,
            favorites_view: FavoriteView::default(),
            browsing_folder: None,
            opening: None,
            notice: None,
            user_mid: None,
            list_width: px(0.),
            folder_scroll: ListScroll::new(),
            favorites_keyword: String::new(),
            search,
            cards,
        }
    }

    fn submit_search(&mut self, cx: &mut Context<Self>) {
        if !self.search.submit(cx) {
            return;
        }
        self.cards.clear();
        let folder_contents = self.tab == MineTab::Favorites
            && (self.favorites_view == FavoriteView::Folder || self.browsing_folder.is_some());
        if folder_contents {
            self.load_favorite_items(cx);
        }
        cx.notify();
    }

    pub(crate) fn search_field(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.search.field(cx)
    }

    pub(crate) fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.focus(window, cx);
    }

    fn bangumi_state(&self, cinema: bool) -> &Paged<BangumiFollow, u32> {
        if cinema { &self.cinema } else { &self.bangumi }
    }

    fn bangumi_state_mut(&mut self, cinema: bool) -> &mut Paged<BangumiFollow, u32> {
        if cinema {
            &mut self.cinema
        } else {
            &mut self.bangumi
        }
    }

    fn state(&self, tab: MineTab) -> &Paged<VideoCard, ListMeta> {
        match tab {
            MineTab::History | MineTab::Offline => &self.history,
            MineTab::Favorites => &self.favorites,
            MineTab::WatchLater => &self.watch_later,
        }
    }

    fn show_tab(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.cards.clear();
        preferences::set_mine_tab(cx, tab);
        self.load_if_needed(cx);
        cx.notify();
    }

    fn sync_user(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mid = rigili_core::login::user(cx).map(|user| user.mid);
        if mid == self.user_mid {
            return;
        }

        self.user_mid = mid;
        self.stat = None;
        self.stat_loading = false;
        self.stat_error = None;
        self.favorites_view = FavoriteView::default();
        self.browsing_folder = None;
        self.folders = FolderState::new();
        self.favorites = Paged::new();
        self.history = Paged::new();
        self.watch_later = Paged::new();
        self.collected = CollectedState::new();
        self.bangumi = Paged::new();
        self.cinema = Paged::new();
        self.opening = None;
        self.notice = None;
        self.favorites_keyword.clear();
        self.cards.clear();

        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.search.clear(window, cx);
                this.load_stat(cx);
                this.load_if_needed(cx);
            });
        });
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .child(
                TabStrip::new("mine-tab")
                    .gap(px(16.))
                    .padding_x(px(8.))
                    .render_nav(self, cx, MineTab::label, |_| None),
            )
            .into_any_element()
    }
}

impl TabNav for MineView {
    type Tab = MineTab;

    const TABS: &'static [MineTab] = &MineTab::ALL;

    fn active_tab(&self) -> MineTab {
        self.tab
    }

    fn select_tab(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        self.show_tab(tab, cx);
    }
}

impl CardNav for MineView {
    fn cards(&self) -> &CardCursor {
        &self.cards
    }

    fn cards_mut(&mut self) -> &mut CardCursor {
        &mut self.cards
    }

    fn card_count(&self) -> usize {
        let Some(tab) = self.card_tab() else {
            return 0;
        };
        let items = &self.state(tab).items;
        match self.card_needle(tab) {
            Some(needle) => items.iter().filter(|card| card_matches(card, &needle)).count(),
            None => items.len(),
        }
    }

    fn card_columns(&self) -> usize {
        VideoCards::columns(self.list_width)
    }

    fn reveal_card(&self, index: usize) {
        let Some(tab) = self.card_tab() else {
            return;
        };
        let columns = self.card_columns().max(1);
        self.state(tab)
            .scroll
            .scroll_to_item(index / columns, ScrollStrategy::Nearest);
    }

    fn open_card(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(tab) = self.card_tab() else {
            return;
        };
        let Some(displayed) = self.cards.index() else {
            return;
        };
        let Some(index) = self.card_item_index(tab, displayed) else {
            return;
        };
        let Some(card) = self.state(tab).items.get(index) else {
            return;
        };
        let mut open = card.open();
        open.new_tab = new_tab;
        cx.emit(open);
    }
}

impl MineView {
    fn card_tab(&self) -> Option<MineTab> {
        match self.tab {
            MineTab::History | MineTab::WatchLater => Some(self.tab),
            MineTab::Favorites => match self.favorites_view {
                FavoriteView::Folder => Some(MineTab::Favorites),
                FavoriteView::Folders | FavoriteView::Subscribed
                    if self.browsing_folder.is_some() =>
                {
                    Some(MineTab::Favorites)
                }
                _ => None,
            },
            MineTab::Offline => None,
        }
    }

    fn card_needle(&self, tab: MineTab) -> Option<String> {
        if tab == MineTab::Favorites {
            None
        } else {
            self.search.needle()
        }
    }

    fn card_item_index(&self, tab: MineTab, displayed: usize) -> Option<usize> {
        let needle = self.card_needle(tab);
        card_item_index(&self.state(tab).items, needle.as_deref(), displayed)
    }
}

fn card_item_index(
    cards: &[VideoCard],
    needle: Option<&str>,
    displayed: usize,
) -> Option<usize> {
    match needle {
        Some(needle) => cards
            .iter()
            .enumerate()
            .filter(|(_, card)| card_matches(card, needle))
            .nth(displayed)
            .map(|(index, _)| index),
        None => (displayed < cards.len()).then_some(displayed),
    }
}

fn card_matches(card: &VideoCard, needle: &str) -> bool {
    filter::text_matches(card.title(), needle) || filter::text_matches(card.author(), needle)
}

impl Refresh for MineView {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.stat = None;
        self.stat_loading = false;
        self.stat_error = None;

        match self.tab {
            MineTab::Favorites => {
                if self.browsing_folder.is_some() {
                    self.load_favorite_items(cx);
                } else {
                    match self.favorites_view {
                        FavoriteView::Folder | FavoriteView::Folders => self.load_folders(cx),
                        FavoriteView::Subscribed => self.load_collected(cx),
                        FavoriteView::Bangumi => self.load_bangumi(false, cx),
                        FavoriteView::Cinema => self.load_bangumi(true, cx),
                    }
                }
            }
            MineTab::History => self.load_history(cx),
            MineTab::WatchLater => self.load_watch_later(cx),
            MineTab::Offline => {}
        }
        self.load_stat(cx);
    }

    fn can_refresh(&self) -> bool {
        self.user_mid.is_some() && self.tab != MineTab::Offline
    }
}

impl Render for MineView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_user(window, cx);

        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let show_body = self.user_mid.is_some();
        let profile = self.render_profile(cx);
        let notice = self.notice.clone();

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .child(profile)
            .when_some(notice, |this, notice| {
                this.child(
                    Alert::error("mine-notice", notice)
                        .title("打开失败")
                        .on_close(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                )
            })
            .when(show_body, |this| {
                this.child(self.render_tabs(cx))
                    .child(self.render_body(window, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use rigili_bilibili::SearchVideo;

    use super::card_item_index;
    use crate::shared::browse::grid::VideoCard;

    fn card(title: &str) -> VideoCard {
        VideoCard::from(SearchVideo {
            bvid: title.into(),
            title: title.into(),
            pic: String::new(),
            author: String::new(),
            mid: 0,
            duration: 0,
            pubdate: 0,
            play: 0,
            danmaku: 0,
            like: 0,
        })
    }

    #[test]
    fn maps_filtered_card_positions_to_items() {
        let cards = [card("Rust 入门"), card("Go 入门"), card("Rust 进阶")];

        assert_eq!(card_item_index(&cards, Some("rust"), 0), Some(0));
        assert_eq!(card_item_index(&cards, Some("rust"), 1), Some(2));
        assert_eq!(card_item_index(&cards, Some("rust"), 2), None);

        assert_eq!(card_item_index(&cards, None, 1), Some(1));
        assert_eq!(card_item_index(&cards, None, 3), None);
    }
}
