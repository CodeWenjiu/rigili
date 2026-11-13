rigili_macro::mod_prv!(create_folder, feed, list, sort_folders);

use std::ops::{Deref, DerefMut};

use gpui_kit::assets::IconName;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{BangumiFollow, FavFolder, NavStat};
use rigili_core::backend::{self, Backend};
use rigili_core::{OpenSpace, OpenVideo, Refresh, format_count};

use crate::filter;
use crate::grid::VideoCard;
use crate::paging::Paged;
use crate::profile::{profile_stat, profile_title};
use crate::scroll;
use crate::tabs::TabStrip;
use create_folder::CreateFolderDialog;
use sort_folders::SortFoldersDialog;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

pub(crate) struct VideoListState {
    list: Paged<VideoCard, ListMeta>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ListMeta {
    pub(crate) page: u32,
    pub(crate) max: u64,
    pub(crate) view_at: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FavoriteView {
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

impl VideoListState {
    fn new() -> Self {
        Self { list: Paged::new() }
    }
}

impl Deref for VideoListState {
    type Target = Paged<VideoCard, ListMeta>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for VideoListState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
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

pub(crate) struct BangumiState(Paged<BangumiFollow, u32>);

impl BangumiState {
    fn new() -> Self {
        Self(Paged::new())
    }
}

impl Deref for BangumiState {
    type Target = Paged<BangumiFollow, u32>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BangumiState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct MineView {
    backend: Option<Backend>,
    tab: MineTab,
    folders: FolderState,
    favorites: VideoListState,
    history: VideoListState,
    watch_later: VideoListState,
    collected: CollectedState,
    bangumi: BangumiState,
    cinema: BangumiState,
    stat: Option<NavStat>,
    stat_loading: bool,
    stat_error: Option<String>,
    favorites_view: FavoriteView,
    browsing_folder: Option<FavoriteView>,
    opening: Option<u64>,
    notice: Option<String>,
    user_mid: Option<u64>,
    list_width: Pixels,
    folder_scroll: scroll::ScrollArea,
    favorites_keyword: String,
    search: filter::Search,
}

impl EventEmitter<OpenVideo> for MineView {}
impl EventEmitter<OpenSpace> for MineView {}

impl crate::list::ListWidth for MineView {
    fn list_width(&self) -> Pixels {
        self.list_width
    }

    fn set_list_width(&mut self, width: Pixels) {
        self.list_width = width;
    }
}

impl crate::season::SeasonOpener for MineView {
    fn opening(&mut self) -> &mut Option<u64> {
        &mut self.opening
    }

    fn notice(&mut self) -> &mut Option<String> {
        &mut self.notice
    }

    fn season_backend(&self) -> Option<Backend> {
        self.backend.clone()
    }
}

impl MineView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let backend = backend::get(cx).ok();
        let search =
            filter::Search::new(window, cx, "搜索收藏、历史与追番", |this, cx| {
                this.submit_search(cx)
            });
        Self {
            backend,
            tab: MineTab::default(),
            folders: FolderState::new(),
            favorites: VideoListState::new(),
            history: VideoListState::new(),
            watch_later: VideoListState::new(),
            collected: CollectedState::new(),
            bangumi: BangumiState::new(),
            cinema: BangumiState::new(),
            stat: None,
            stat_loading: false,
            stat_error: None,
            favorites_view: FavoriteView::default(),
            browsing_folder: None,
            opening: None,
            notice: None,
            user_mid: None,
            list_width: px(0.),
            folder_scroll: scroll::ScrollArea::new(),
            favorites_keyword: String::new(),
            search,
        }
    }

    fn submit_search(&mut self, cx: &mut Context<Self>) {
        if !self.search.submit(cx) {
            return;
        }
        let folder_contents = self.tab == MineTab::Favorites
            && (self.favorites_view == FavoriteView::Folder || self.browsing_folder.is_some());
        if folder_contents {
            self.load_favorite_items(cx);
        }
        cx.notify();
    }

    fn bangumi_state(&self, cinema: bool) -> &BangumiState {
        if cinema { &self.cinema } else { &self.bangumi }
    }

    fn bangumi_state_mut(&mut self, cinema: bool) -> &mut BangumiState {
        if cinema {
            &mut self.cinema
        } else {
            &mut self.bangumi
        }
    }

    fn state(&self, tab: MineTab) -> &VideoListState {
        match tab {
            MineTab::History | MineTab::Offline => &self.history,
            MineTab::Favorites => &self.favorites,
            MineTab::WatchLater => &self.watch_later,
        }
    }

    pub(crate) fn show_tab(&mut self, tab: MineTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
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
        self.favorites = VideoListState::new();
        self.history = VideoListState::new();
        self.watch_later = VideoListState::new();
        self.collected = CollectedState::new();
        self.bangumi = BangumiState::new();
        self.cinema = BangumiState::new();
        self.opening = None;
        self.notice = None;
        self.favorites_keyword.clear();

        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.search.clear(window, cx);
                this.load_stat(cx);
                this.load_if_needed(cx);
            });
        });
    }

    fn render_profile(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let Some(user) = rigili_core::login::user(cx) else {
            return crate::login::Prompt::new(
                "mine",
                IconName::CircleUser,
                "登录后查看历史记录、收藏和稍后再看",
            )
            .title("未登录")
            .render(cx);
        };

        let stat = self.stat.as_ref();
        let stat_value = |value: Option<u64>| match value {
            Some(value) => format_count(value),
            None => "—".into(),
        };
        let dynamic = stat_value(stat.map(|stat| stat.dynamic_count));
        let following = stat_value(stat.map(|stat| stat.following));
        let follower = stat_value(stat.map(|stat| stat.follower));
        let balance = format!(
            "B币: {}　硬币: {}",
            format_balance(user.bcoins().unwrap_or(0.)),
            format_balance(user.coins().unwrap_or(0.))
        );
        let mid = user.mid;
        let space_name = user.uname.clone();

        h_flex()
            .w_full()
            .p_5()
            .gap_5()
            .items_center()
            .rounded(theme.radius_lg)
            .bg(theme.muted.opacity(0.4))
            .child(
                Avatar::new()
                    .src(user.face.clone())
                    .name(user.uname.clone())
                    .size(px(64.)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(profile_title(
                        user.uname.clone(),
                        user.level(),
                        user.vip_label().map(str::to_owned),
                        None,
                        cx,
                    ))
                    .child(div().text_xs().text_color(muted).child(balance)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_6()
                    .child(profile_stat("动态", dynamic, cx))
                    .child(profile_stat("关注", following, cx))
                    .child(profile_stat("粉丝", follower, cx)),
            )
            .child(
                Button::new("mine-open-space")
                    .text()
                    .small()
                    .text_color(muted)
                    .label("个人空间 >")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(OpenSpace {
                            mid,
                            name: space_name.clone(),
                        });
                    })),
            )
            .into_any_element()
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
                    .render(
                        cx,
                        &MineTab::ALL,
                        self.tab,
                        MineTab::label,
                        |_| None,
                        |this, tab, cx| this.show_tab(tab, cx),
                    ),
            )
            .child(div().flex_1())
            .child(self.search.field())
            .into_any_element()
    }

    pub(super) fn open_season(&mut self, season_id: u64, cx: &mut Context<Self>) {
        crate::season::SeasonOpener::open_season(self, season_id, cx);
    }

    fn current_folder(&self) -> Option<&FavFolder> {
        let id = self.folders.current?;
        self.folders
            .folders
            .iter()
            .chain(self.collected.folders.iter())
            .find(|folder| folder.id == id)
    }

    fn show_favorite_view(&mut self, view: FavoriteView, cx: &mut Context<Self>) {
        if self.favorites_view == view {
            if self.browsing_folder.is_some() {
                self.browsing_folder = None;
                cx.notify();
            }
            return;
        }
        self.favorites_view = view;
        self.browsing_folder = None;
        if view == FavoriteView::Folder {
            self.ensure_default_folder(cx);
            if self.favorites_keyword != self.search.keyword() {
                self.load_favorite_items(cx);
            }
        } else {
            self.load_favorite_view(cx);
        }
        cx.notify();
    }

    fn default_folder_id(&self) -> Option<u64> {
        self.folders
            .folders
            .iter()
            .find(|folder| folder.is_default())
            .map(|folder| folder.id)
            .or_else(|| self.folders.folders.first().map(|folder| folder.id))
    }

    fn ensure_default_folder(&mut self, cx: &mut Context<Self>) {
        let default_id = self.default_folder_id();
        if self.folders.current == default_id {
            return;
        }
        self.folders.current = default_id;
        if default_id.is_some() {
            self.load_favorite_items(cx);
        } else {
            self.favorites = VideoListState::new();
        }
    }

    fn close_folder(&mut self, cx: &mut Context<Self>) {
        self.browsing_folder = None;
        cx.notify();
    }

    fn load_favorite_view(&mut self, cx: &mut Context<Self>) {
        if self.browsing_folder.is_some() && self.favorites_keyword != self.search.keyword() {
            self.load_favorite_items(cx);
            return;
        }
        match self.favorites_view {
            FavoriteView::Folder if self.favorites_keyword != self.search.keyword() => {
                self.load_favorite_items(cx);
            }
            FavoriteView::Subscribed
                if !self.collected.loaded
                    && !self.collected.loading
                    && self.collected.error.is_none() =>
            {
                self.load_collected(cx);
            }
            FavoriteView::Bangumi
                if !self.bangumi.loaded
                    && !self.bangumi.loading
                    && self.bangumi.error.is_none() =>
            {
                self.load_bangumi(false, cx);
            }
            FavoriteView::Cinema
                if !self.cinema.loaded && !self.cinema.loading && self.cinema.error.is_none() =>
            {
                self.load_bangumi(true, cx);
            }
            _ => {}
        }
    }

    fn render_favorite_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(FavoriteView::ALL.map(|view| {
                let active = view == self.favorites_view;
                let group = SharedString::from(format!("mine-favorite-view-{}", view.index()));
                let label: String = match view {
                    FavoriteView::Folder => "默认收藏夹".into(),
                    FavoriteView::Folders => "我创建的收藏夹".into(),
                    FavoriteView::Subscribed => "我的收藏与订阅".into(),
                    FavoriteView::Bangumi => "我的追番".into(),
                    FavoriteView::Cinema => "我的追剧".into(),
                };

                v_flex()
                    .group(group.clone())
                    .items_center()
                    .rounded(radius)
                    .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                    .child(
                        Button::new(("mine-favorite-view", view.index()))
                            .text()
                            .small()
                            .px_2()
                            .accessibility_label(label.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.show_favorite_view(view, cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if active { primary } else { muted })
                                    .group_hover(group, move |this| this.text_color(primary))
                                    .child(label),
                            ),
                    )
            }))
            .when(self.favorites_view == FavoriteView::Folders, |this| {
                this.child(div().flex_1()).child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_2()
                        .child(
                            Button::new("mine-create-folder")
                                .small()
                                .secondary()
                                .label("+ 新建收藏夹")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_create_folder(window, cx);
                                })),
                        )
                        .when(
                            self.folders
                                .folders
                                .iter()
                                .any(|folder| !folder.is_default()),
                            |this| {
                                this.child(
                                    Button::new("mine-sort-folders")
                                        .small()
                                        .ghost()
                                        .label("收藏夹排序")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_sort_folders(window, cx);
                                        })),
                                )
                            },
                        ),
                )
            })
            .into_any_element()
    }

    fn open_create_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mine = cx.entity();
        let view = cx.new(|cx| CreateFolderDialog::new(window, cx, mine));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(320.)).title("新建收藏夹").child(view.clone())
        });
    }

    fn open_sort_folders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folders = self
            .folders
            .folders
            .iter()
            .filter(|folder| !folder.is_default())
            .cloned()
            .collect::<Vec<_>>();
        if folders.is_empty() {
            return;
        }
        let default_id = self
            .folders
            .folders
            .iter()
            .find(|folder| folder.is_default())
            .map(|folder| folder.id);
        let mine = cx.entity();
        let view = cx.new(|_| SortFoldersDialog::new(window, mine, folders, default_id));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(360.)).title("收藏夹排序").child(view.clone())
        });
    }

    pub(super) fn folders_changed(&mut self, cx: &mut Context<Self>) {
        self.favorites_view = FavoriteView::Folders;
        self.browsing_folder = None;
        self.load_folders(cx);
        cx.notify();
    }

    fn render_folder_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let title = self
            .current_folder()
            .map(|folder| folder.title.clone())
            .unwrap_or_default();

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("mine-folder-back")
                            .text()
                            .small()
                            .text_color(muted)
                            .label("← 返回")
                            .on_click(cx.listener(|this, _, _, cx| this.close_folder(cx))),
                    )
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title)),
            )
            .child(self.render_content(MineTab::Favorites, window, cx))
            .into_any_element()
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.tab == MineTab::Offline {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("离线缓存暂未支持")
                .into_any_element();
        }

        let content = match self.tab {
            MineTab::Favorites => match self.favorites_view {
                FavoriteView::Folder => self.render_content(MineTab::Favorites, window, cx),
                FavoriteView::Folders => {
                    if self.browsing_folder == Some(FavoriteView::Folders) {
                        self.render_folder_detail(window, cx)
                    } else {
                        self.render_folder_grid(FolderSource::Created, window, cx)
                    }
                }
                FavoriteView::Subscribed => {
                    if self.browsing_folder == Some(FavoriteView::Subscribed) {
                        self.render_folder_detail(window, cx)
                    } else {
                        self.render_folder_grid(FolderSource::Collected, window, cx)
                    }
                }
                FavoriteView::Bangumi => self.render_bangumi(false, window, cx),
                FavoriteView::Cinema => self.render_bangumi(true, window, cx),
            },
            _ => self.render_content(self.tab, window, cx),
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .when(self.tab == MineTab::Favorites, |this| {
                this.child(self.render_favorite_tabs(cx))
            })
            .child(content)
            .into_any_element()
    }
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

fn format_balance(value: f64) -> String {
    if (value.fract()).abs() < 0.05 {
        format!("{}", value.round() as u64)
    } else {
        format!("{value:.1}")
    }
}
