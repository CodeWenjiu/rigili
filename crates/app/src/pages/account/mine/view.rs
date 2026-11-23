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
use crate::shared::browse::grid::{
    BangumiCards, CardFrame, CardGrid, FolderCards, VideoCard, VideoCards,
};
use crate::shared::browse::list::ListScroll;
use crate::shared::browse::paging::Paged;
use crate::shared::browse::season::SeasonOpener;
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
        let search =
            filter::Search::new(window, cx, "搜索收藏、历史与追番", |this, cx| {
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
        let Some(grid) = self.card_grid() else {
            return 0;
        };
        match grid {
            CardGridSource::Videos(tab) => self
                .video_indices(tab)
                .map_or_else(|| self.state(tab).items.len(), |indices| indices.len()),
            CardGridSource::Folders(source) => self.folder_indices(source).len(),
            CardGridSource::Bangumi(cinema) => self.bangumi_indices(cinema).map_or_else(
                || self.bangumi_state(cinema).items.len(),
                |indices| indices.len(),
            ),
        }
    }

    fn card_columns(&self) -> usize {
        match self.card_grid() {
            Some(CardGridSource::Folders(_)) => FolderCards::columns(self.list_width),
            Some(CardGridSource::Bangumi(_)) => BangumiCards::columns(self.list_width),
            _ => VideoCards::columns(self.list_width),
        }
    }

    fn reveal_card(&self, index: usize) {
        let Some(grid) = self.card_grid() else {
            return;
        };
        let columns = self.card_columns().max(1);
        let row = index / columns;
        match grid {
            CardGridSource::Videos(tab) => self
                .state(tab)
                .scroll
                .scroll_to_item(row, ScrollStrategy::Nearest),
            CardGridSource::Folders(_) => self
                .folder_scroll
                .scroll_to_item(row, ScrollStrategy::Nearest),
            CardGridSource::Bangumi(cinema) => self
                .bangumi_state(cinema)
                .scroll
                .scroll_to_item(row, ScrollStrategy::Nearest),
        }
    }

    fn open_card(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(displayed) = self.cards.index() else {
            return;
        };
        let Some(grid) = self.card_grid() else {
            return;
        };
        match grid {
            CardGridSource::Videos(tab) => {
                let needle = self.card_needle(tab);
                let Some(index) =
                    card_item_index(&self.state(tab).items, needle.as_deref(), displayed)
                else {
                    return;
                };
                let Some(card) = self.state(tab).items.get(index) else {
                    return;
                };
                let mut open = card.open();
                open.new_tab = new_tab;
                cx.emit(open);
            }
            CardGridSource::Folders(source) => {
                let indices = self.folder_indices(source);
                let Some(&index) = indices.get(displayed) else {
                    return;
                };
                let Some(folder) = self.folder_items(source).get(index) else {
                    return;
                };
                let id = folder.id;
                self.open_folder(id, source, cx);
            }
            CardGridSource::Bangumi(cinema) => {
                let indices = self.bangumi_indices(cinema);
                let len = self.bangumi_state(cinema).items.len();
                let Some(index) = filtered_item(indices.as_deref(), len, displayed) else {
                    return;
                };
                let Some(item) = self.bangumi_state(cinema).items.get(index) else {
                    return;
                };
                let season_id = item.season_id;
                self.open_season(season_id, cx);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardGridSource {
    Videos(MineTab),
    Folders(FolderSource),
    Bangumi(bool),
}

fn card_grid_for(
    tab: MineTab,
    view: FavoriteView,
    browsing_folder: bool,
) -> Option<CardGridSource> {
    match tab {
        MineTab::History | MineTab::WatchLater => Some(CardGridSource::Videos(tab)),
        MineTab::Offline => None,
        MineTab::Favorites => match view {
            FavoriteView::Folder => Some(CardGridSource::Videos(MineTab::Favorites)),
            FavoriteView::Folders if browsing_folder => {
                Some(CardGridSource::Videos(MineTab::Favorites))
            }
            FavoriteView::Folders => Some(CardGridSource::Folders(FolderSource::Created)),
            FavoriteView::Subscribed if browsing_folder => {
                Some(CardGridSource::Videos(MineTab::Favorites))
            }
            FavoriteView::Subscribed => Some(CardGridSource::Folders(FolderSource::Collected)),
            FavoriteView::Bangumi => Some(CardGridSource::Bangumi(false)),
            FavoriteView::Cinema => Some(CardGridSource::Bangumi(true)),
        },
    }
}

impl MineView {
    fn card_grid(&self) -> Option<CardGridSource> {
        card_grid_for(
            self.tab,
            self.favorites_view,
            self.browsing_folder.is_some(),
        )
    }

    fn card_needle(&self, tab: MineTab) -> Option<String> {
        if tab == MineTab::Favorites {
            None
        } else {
            self.search.needle()
        }
    }

    fn video_indices(&self, tab: MineTab) -> Option<Vec<usize>> {
        let needle = self.card_needle(tab);
        filter::filter_indices(&self.state(tab).items, needle.as_deref(), |card, needle| {
            card_matches(card, needle)
        })
    }

    fn folder_items(&self, source: FolderSource) -> &[FavFolder] {
        match source {
            FolderSource::Created => &self.folders.folders,
            FolderSource::Collected => &self.collected.folders,
        }
    }

    fn folder_indices(&self, source: FolderSource) -> Vec<usize> {
        let needle = self.search.needle();
        filter::matching(
            self.folder_items(source)
                .iter()
                .enumerate()
                .filter(|(_, folder)| match source {
                    FolderSource::Created => !folder.is_default(),
                    FolderSource::Collected => folder.is_valid(),
                }),
            needle.as_deref(),
            |item, needle| {
                let (_, folder) = item;
                filter::text_matches(&folder.title, needle)
                    || (source == FolderSource::Collected
                        && filter::text_matches(folder.owner(), needle))
            },
        )
        .into_iter()
        .map(|(index, _)| index)
        .collect()
    }

    fn bangumi_indices(&self, cinema: bool) -> Option<Vec<usize>> {
        let needle = self.search.needle();
        filter::filter_indices(
            &self.bangumi_state(cinema).items,
            needle.as_deref(),
            |item, needle| filter::text_matches(&item.title, needle),
        )
    }
}

fn filtered_item(indices: Option<&[usize]>, len: usize, displayed: usize) -> Option<usize> {
    match indices {
        Some(indices) => indices.get(displayed).copied(),
        None => (displayed < len).then_some(displayed),
    }
}

fn card_item_index(cards: &[VideoCard], needle: Option<&str>, displayed: usize) -> Option<usize> {
    let indices = filter::filter_indices(cards, needle, card_matches);
    filtered_item(indices.as_deref(), cards.len(), displayed)
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

    use super::{
        CardGridSource, FavoriteView, FolderSource, MineTab, card_grid_for, card_item_index,
    };
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

    #[test]
    fn routes_each_mine_view_to_its_card_grid() {
        assert_eq!(
            card_grid_for(MineTab::History, FavoriteView::Folder, false),
            Some(CardGridSource::Videos(MineTab::History))
        );
        assert_eq!(
            card_grid_for(MineTab::WatchLater, FavoriteView::Folder, false),
            Some(CardGridSource::Videos(MineTab::WatchLater))
        );
        assert_eq!(
            card_grid_for(MineTab::Offline, FavoriteView::Folder, false),
            None
        );

        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Folder, false),
            Some(CardGridSource::Videos(MineTab::Favorites))
        );
        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Folders, false),
            Some(CardGridSource::Folders(FolderSource::Created))
        );
        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Folders, true),
            Some(CardGridSource::Videos(MineTab::Favorites))
        );
        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Subscribed, false),
            Some(CardGridSource::Folders(FolderSource::Collected))
        );
        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Subscribed, true),
            Some(CardGridSource::Videos(MineTab::Favorites))
        );
        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Bangumi, false),
            Some(CardGridSource::Bangumi(false))
        );
        assert_eq!(
            card_grid_for(MineTab::Favorites, FavoriteView::Cinema, false),
            Some(CardGridSource::Bangumi(true))
        );
    }
}
