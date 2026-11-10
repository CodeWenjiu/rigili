rigili_macro::mod_prv!(feed, list);

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{FavFolder, NavStat};

use crate::backend::{self, Backend};
use crate::format::format_count;
use crate::grid::{OpenVideo, VideoCard};
use crate::login;
use crate::refresh::Refresh;

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
    items: Vec<VideoCard>,
    page: u32,
    cursor_max: u64,
    cursor_view_at: i64,
    has_more: bool,
    loading: bool,
    loading_more: bool,
    loaded: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
    scroll: VirtualListScrollHandle,
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
        Self {
            items: Vec::new(),
            page: 0,
            cursor_max: 0,
            cursor_view_at: 0,
            has_more: false,
            loading: false,
            loading_more: false,
            loaded: false,
            error: None,
            more_error: None,
            generation: 0,
            scroll: VirtualListScrollHandle::new(),
        }
    }

    fn reset(&mut self) -> u64 {
        self.generation += 1;
        self.items.clear();
        self.page = 0;
        self.cursor_max = 0;
        self.cursor_view_at = 0;
        self.has_more = false;
        self.loading = true;
        self.loading_more = false;
        self.loaded = false;
        self.error = None;
        self.more_error = None;
        self.generation
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

pub struct MineView {
    backend: Option<Backend>,
    tab: MineTab,
    folders: FolderState,
    favorites: VideoListState,
    history: VideoListState,
    watch_later: VideoListState,
    stat: Option<NavStat>,
    stat_loading: bool,
    stat_error: Option<String>,
    favorites_view: FavoriteView,
    user_mid: Option<u64>,
    list_width: Pixels,
}

impl EventEmitter<OpenVideo> for MineView {}

impl MineView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let backend = backend::get(cx).ok();
        Self {
            backend,
            tab: MineTab::default(),
            folders: FolderState::new(),
            favorites: VideoListState::new(),
            history: VideoListState::new(),
            watch_later: VideoListState::new(),
            stat: None,
            stat_loading: false,
            stat_error: None,
            favorites_view: FavoriteView::default(),
            user_mid: None,
            list_width: px(0.),
        }
    }

    fn state(&self, tab: MineTab) -> &VideoListState {
        match tab {
            MineTab::History => &self.history,
            MineTab::Favorites => &self.favorites,
            MineTab::WatchLater => &self.watch_later,
            MineTab::Offline => &self.history,
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
        let mid = login::user(cx).map(|user| user.mid);
        if mid == self.user_mid {
            return;
        }

        self.user_mid = mid;
        self.stat = None;
        self.stat_loading = false;
        self.stat_error = None;
        self.favorites_view = FavoriteView::default();
        self.folders = FolderState::new();
        self.favorites = VideoListState::new();
        self.history = VideoListState::new();
        self.watch_later = VideoListState::new();

        let weak = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.load_stat(cx);
                this.load_if_needed(cx);
            });
        });
    }

    fn render_profile(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let primary_foreground = theme.primary_foreground;

        let Some(user) = login::user(cx) else {
            return h_flex()
                .w_full()
                .p_6()
                .gap_5()
                .items_center()
                .rounded(theme.radius_lg)
                .bg(theme.muted.opacity(0.4))
                .child(
                    div()
                        .size(px(64.))
                        .rounded_full()
                        .bg(theme.accent)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::CircleUser)
                                .with_size(px(32.))
                                .text_color(muted),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(div().text_lg().child("未登录"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child("登录后查看历史记录、收藏和稍后再看"),
                        ),
                )
                .child(
                    Button::new("mine-sign-in")
                        .primary()
                        .label("登录")
                        .on_click(cx.listener(|_, _, window, cx| {
                            login::open(window, cx);
                        })),
                )
                .into_any_element();
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
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_lg()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(user.uname.clone()),
                            )
                            .when_some(user.level(), |this, level| {
                                this.child(profile_badge(
                                    format!("LV{level}"),
                                    primary.opacity(0.15),
                                    primary,
                                ))
                            })
                            .when_some(user.vip_label().map(str::to_owned), |this, label| {
                                this.child(profile_badge(label, primary, primary_foreground))
                            }),
                    )
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
                        cx.open_url(&format!("https://space.bilibili.com/{mid}"));
                    })),
            )
            .into_any_element()
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .items_end()
            .gap_4()
            .border_b_1()
            .border_color(border)
            .children(MineTab::ALL.map(|tab| {
                let active = tab == self.tab;
                let group = SharedString::from(format!("mine-tab-{}", tab.index()));

                v_flex()
                    .group(group.clone())
                    .items_center()
                    .gap_1()
                    .rounded(radius)
                    .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                    .child(
                        Button::new(("mine-tab", tab.index()))
                            .text()
                            .px_2()
                            .py_1()
                            .accessibility_label(tab.label())
                            .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if active { primary } else { muted })
                                    .group_hover(group, move |this| this.text_color(primary))
                                    .child(tab.label()),
                            ),
                    )
                    .child(div().h(px(2.)).w(px(24.)).rounded(px(1.)).bg(if active {
                        primary
                    } else {
                        hsla(0., 0., 0., 0.)
                    }))
            }))
            .into_any_element()
    }

    fn current_folder(&self) -> Option<&FavFolder> {
        let id = self.folders.current?;
        self.folders.folders.iter().find(|folder| folder.id == id)
    }

    fn show_favorite_view(&mut self, view: FavoriteView, cx: &mut Context<Self>) {
        if self.favorites_view == view {
            return;
        }
        self.favorites_view = view;
        cx.notify();
    }

    fn render_favorite_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;
        let current_title = self
            .current_folder()
            .map(|folder| folder.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "默认收藏夹".into());

        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(FavoriteView::ALL.map(|view| {
                let active = view == self.favorites_view;
                let group = SharedString::from(format!("mine-favorite-view-{}", view.index()));
                let label = match view {
                    FavoriteView::Folder => current_title.clone(),
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

        let content = if self.tab == MineTab::Favorites {
            match self.favorites_view {
                FavoriteView::Folder => self.render_content(MineTab::Favorites, window, cx),
                FavoriteView::Folders => self.render_folder_grid(cx),
                FavoriteView::Subscribed | FavoriteView::Bangumi | FavoriteView::Cinema => div()
                    .flex_1()
                    .min_h_0()
                    .text_sm()
                    .text_color(muted)
                    .child("暂未支持")
                    .into_any_element(),
            }
        } else {
            self.render_content(self.tab, window, cx)
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
            MineTab::Favorites => self.load_folders(cx),
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
        let logged_in = self.user_mid.is_some();
        let profile = self.render_profile(cx);

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .child(profile)
            .when(logged_in, |this| {
                this.child(self.render_tabs(cx))
                    .child(self.render_body(window, cx))
            })
    }
}

fn profile_badge(label: impl Into<SharedString>, background: Hsla, foreground: Hsla) -> AnyElement {
    div()
        .flex_shrink_0()
        .px_2()
        .py_1()
        .rounded(px(4.))
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .child(label.into())
        .into_any_element()
}

fn profile_stat(label: &'static str, value: String, cx: &App) -> AnyElement {
    v_flex()
        .items_center()
        .gap_1()
        .child(div().text_lg().child(value))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .into_any_element()
}

fn format_balance(value: f64) -> String {
    if (value.fract()).abs() < 0.05 {
        format!("{}", value.round() as u64)
    } else {
        format!("{value:.1}")
    }
}
