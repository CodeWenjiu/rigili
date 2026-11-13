use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::space::SpaceVideoOrder;
use rigili_bilibili::{
    DynamicArchive, DynamicItem, SeasonMeta, SeriesMeta, SpaceProfile, banner, thumbnail,
};
use rigili_core::backend::{self, Backend};
use rigili_core::login as login_state;
use rigili_core::runtime::spawn_api;
use rigili_core::{OpenSpace, OpenVideo, Refresh, api_task_error_message, format_count};

use super::{comment, compose};
use crate::filter::{self, text_matches};
use crate::grid::{
    CARD_GAP, CardFrame, CardGrid, CardStyle, FolderCards, VideoCard, VideoCards, card_text_height,
};
use crate::list::{
    LIST_FOOTER_HEIGHT, Rows, list_footer, track_element_width, virtual_rows_tracked,
};
use crate::login;
use crate::paging::{Page, Paged, spawn_first, spawn_more};
use crate::profile::{profile_stat, profile_title};
use crate::scroll;
use crate::state_view::{Layout, centered_spinner, empty_state, error_state};
use crate::tabs::TabStrip;

const DYNAMIC_CARD_MAX_WIDTH: Pixels = px(840.);

#[derive(Clone, Copy)]
pub(crate) struct CloseSpace;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SpaceTab {
    Dynamics,
    #[default]
    Videos,
    Collections,
}

impl SpaceTab {
    const ALL: [Self; 3] = [Self::Dynamics, Self::Videos, Self::Collections];

    fn label(self) -> &'static str {
        match self {
            Self::Dynamics => "动态",
            Self::Videos => "投稿",
            Self::Collections => "合集和系列",
        }
    }
}

pub struct SpaceView {
    backend: Option<Backend>,
    mid: Option<u64>,
    name: String,
    tab: SpaceTab,
    profile: Option<SpaceProfile>,
    profile_loading: bool,
    profile_error: Option<String>,
    followed: bool,
    follow_pending: bool,
    expanded_comments: Option<String>,
    comment_views: HashMap<String, Entity<comment::CommentsView>>,
    comment_trigger_key: Option<String>,
    comments_focus: FocusHandle,
    comment_trigger_focus: FocusHandle,
    dynamics: Paged<DynamicItem, String>,
    space_videos: SpaceVideosState,
    collections: CollectionsState,
    dynamics_scroll: scroll::ScrollArea,
    collections_scroll: scroll::ScrollArea,
    list_width: Pixels,
    search: filter::Search,
}

impl EventEmitter<OpenVideo> for SpaceView {}
impl EventEmitter<OpenSpace> for SpaceView {}
impl EventEmitter<CloseSpace> for SpaceView {}

impl crate::list::ListWidth for SpaceView {
    fn list_width(&self) -> Pixels {
        self.list_width
    }

    fn set_list_width(&mut self, width: Pixels) {
        self.list_width = width;
    }
}

impl SpaceView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search =
            filter::Search::new(window, cx, "搜索动态、投稿、合集", |this, cx| {
                this.submit_search(cx)
            });
        let comments_focus = cx.focus_handle();
        let comment_trigger_focus = cx.focus_handle();
        Self {
            backend: None,
            mid: None,
            name: String::new(),
            tab: SpaceTab::default(),
            profile: None,
            profile_loading: false,
            profile_error: None,
            followed: false,
            follow_pending: false,
            expanded_comments: None,
            comment_views: HashMap::new(),
            comment_trigger_key: None,
            comments_focus,
            comment_trigger_focus,
            dynamics: Paged::new(),
            space_videos: SpaceVideosState::new(),
            collections: CollectionsState::new(),
            dynamics_scroll: scroll::ScrollArea::new(),
            collections_scroll: scroll::ScrollArea::new(),
            list_width: px(0.),
            search,
        }
    }

    pub(crate) fn open(
        &mut self,
        mid: u64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mid = Some(mid);
        self.name = name;
        self.tab = SpaceTab::default();
        self.profile = None;
        self.profile_loading = false;
        self.profile_error = None;
        self.followed = false;
        self.follow_pending = false;
        self.expanded_comments = None;
        self.comment_views.clear();
        self.comment_trigger_key = None;
        self.dynamics = Paged::new();
        self.space_videos = SpaceVideosState::new();
        self.collections = CollectionsState::new();
        self.list_width = px(0.);
        self.search.clear(window, cx);
        self.load_profile(cx);
        self.load_if_needed(cx);
        cx.notify();
    }

    fn backend(&mut self, cx: &mut Context<Self>) -> Result<Backend, String> {
        backend::cached(&mut self.backend, cx)
    }

    fn load_profile(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if self.profile_loading {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.profile_loading = false;
                self.profile_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.profile_loading = true;
        self.profile = None;
        self.profile_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.space_profile(mid).await },
            move |this, result, cx| {
                if this.mid != Some(mid) {
                    return;
                }
                this.profile_loading = false;
                match result {
                    Ok(profile) => {
                        this.followed = profile.followed();
                        this.profile = Some(profile);
                        this.profile_error = None;
                    }
                    Err(error) => this.profile_error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn toggle_follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if self.follow_pending || self.profile.is_none() {
            return;
        }
        let Some(user) = login_state::user(cx) else {
            login::open(window, cx);
            return;
        };
        if Some(user.mid) == self.mid {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                window.push_notification(Notification::error(error), cx);
                return;
            }
        };

        let follow = !self.followed;
        let handle = window.window_handle();
        self.follow_pending = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.modify_relation(mid, follow).await },
            move |this, result, cx| {
                this.follow_pending = false;
                match result {
                    Ok(()) => {
                        if this.mid == Some(mid) {
                            this.apply_follow(follow);
                        }
                        let note = Notification::success(if follow {
                            "已关注"
                        } else {
                            "已取消关注"
                        });
                        let _ = handle.update(cx, |_, window, cx| {
                            window.push_notification(note, cx);
                        });
                    }
                    Err(error) => {
                        let note = Notification::error(api_task_error_message(error, cx));
                        let _ = handle.update(cx, |_, window, cx| {
                            window.push_notification(note, cx);
                        });
                    }
                }
                cx.notify();
            },
        );
    }

    fn apply_follow(&mut self, follow: bool) {
        self.followed = follow;
        if let Some(relation) = self
            .profile
            .as_mut()
            .and_then(|profile| profile.relation.as_mut())
        {
            relation.follower = if follow {
                relation.follower.saturating_add(1)
            } else {
                relation.follower.saturating_sub(1)
            };
        }
    }

    fn open_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if login_state::user(cx).is_none() {
            login::open(window, cx);
            return;
        }
        let name = self
            .profile
            .as_ref()
            .map(|profile| profile.user.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.name.clone());
        compose::open(window, cx, mid, name);
    }

    fn toggle_comments(
        &mut self,
        key: String,
        dynamic: DynamicItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.expanded_comments.as_deref() == Some(key.as_str()) {
            self.collapse_comments(window, cx);
            return;
        }

        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.comment_views.entry(key.clone())
        {
            let view = cx.new(|_| comment::CommentsView::new(dynamic));
            view.update(cx, |view, cx| view.load(true, cx));
            entry.insert(view);
        }

        self.expanded_comments = Some(key.clone());
        self.comment_trigger_key = Some(key);
        cx.notify();
        let focus = self.comments_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
    }

    fn collapse_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.expanded_comments.take().is_none() {
            return;
        }
        cx.notify();
        let focus = self.comment_trigger_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
    }

    fn load_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.mid.is_none() {
            return;
        }
        match self.tab {
            SpaceTab::Dynamics
                if !self.dynamics.loaded
                    && !self.dynamics.loading
                    && self.dynamics.error.is_none() =>
            {
                self.load_dynamics(cx);
            }
            SpaceTab::Videos
                if !self.space_videos.loaded
                    && !self.space_videos.loading
                    && self.space_videos.error.is_none() =>
            {
                self.load_space_videos(cx);
            }
            SpaceTab::Collections
                if !self.collections.loaded
                    && !self.collections.loading
                    && self.collections.error.is_none() =>
            {
                self.load_collections(true, cx);
            }
            _ => {}
        }
    }

    fn reload_tab(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            SpaceTab::Dynamics => self.load_dynamics(cx),
            SpaceTab::Videos => self.load_space_videos(cx),
            SpaceTab::Collections => self.load_collections(true, cx),
        }
    }

    fn show_tab(&mut self, tab: SpaceTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.load_if_needed(cx);
        cx.notify();
    }

    fn submit_search(&mut self, cx: &mut Context<Self>) {
        if !self.search.submit(cx) {
            return;
        }
        self.expanded_comments = None;
        self.comment_trigger_key = None;
        if self.tab == SpaceTab::Videos {
            self.load_space_videos(cx);
        } else {
            self.space_videos = SpaceVideosState::new();
            self.load_if_needed(cx);
            cx.notify();
        }
    }
}

struct SpaceVideosState {
    list: Paged<VideoCard, VideoMeta>,
    order: SpaceVideoOrder,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VideoMeta {
    pub(crate) page: u32,
    pub(crate) total: u64,
}

impl SpaceVideosState {
    fn new() -> Self {
        Self {
            list: Paged::new(),
            order: SpaceVideoOrder::default(),
        }
    }
}

impl Deref for SpaceVideosState {
    type Target = Paged<VideoCard, VideoMeta>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for SpaceVideosState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

struct CollectionsState {
    seasons: Vec<SeasonMeta>,
    series: Vec<SeriesMeta>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
}

impl CollectionsState {
    fn new() -> Self {
        Self {
            seasons: Vec::new(),
            series: Vec::new(),
            loading: false,
            loaded: false,
            error: None,
            generation: 0,
        }
    }

    fn reset(&mut self) -> u64 {
        self.generation += 1;
        self.seasons.clear();
        self.series.clear();
        self.loading = true;
        self.loaded = false;
        self.error = None;
        self.generation
    }
}

impl SpaceView {
    fn load_dynamics(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.dynamics.fail_first_reset(error);
                cx.notify();
                return;
            }
        };

        self.expanded_comments = None;
        self.comment_views.clear();
        self.comment_trigger_key = None;

        spawn_first(
            self,
            cx,
            |this| &mut this.dynamics,
            |item: &DynamicItem| item.id_str.clone(),
            async move {
                let page = backend.dynamics(mid, "").await?;
                Ok(Page::new(page.items, page.has_more, page.offset))
            },
            |_| {},
        );
    }

    fn load_more_dynamics(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if self.dynamics.meta.is_empty() {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.dynamics.fail_more(error);
                cx.notify();
                return;
            }
        };

        spawn_more(
            self,
            cx,
            |this| &mut this.dynamics,
            |item: &DynamicItem| item.id_str.clone(),
            move |offset| async move {
                let page = backend.dynamics(mid, &offset).await?;
                Ok(Page::new(page.items, page.has_more, page.offset))
            },
            |_| {},
        );
    }

    fn retry_more_dynamics(&mut self, cx: &mut Context<Self>) {
        self.dynamics.retry_more();
        self.load_more_dynamics(cx);
    }

    fn load_space_videos(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.space_videos.fail_first_reset(error);
                cx.notify();
                return;
            }
        };
        let order = self.space_videos.order;
        let keyword = self.search.keyword().to_owned();

        spawn_first(
            self,
            cx,
            |this| &mut this.space_videos.list,
            |card: &VideoCard| card.bvid().to_owned(),
            async move { space_page(backend.space_videos(mid, 1, order, &keyword).await?) },
            space_videos_applied,
        );
    }

    fn load_more_space_videos(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.space_videos.fail_more(error);
                cx.notify();
                return;
            }
        };
        let order = self.space_videos.order;
        let keyword = self.search.keyword().to_owned();

        spawn_more(
            self,
            cx,
            |this| &mut this.space_videos.list,
            |card: &VideoCard| card.bvid().to_owned(),
            move |meta| {
                let page = meta.page + 1;
                async move { space_page(backend.space_videos(mid, page, order, &keyword).await?) }
            },
            space_videos_applied,
        );
    }

    fn retry_more_space_videos(&mut self, cx: &mut Context<Self>) {
        self.space_videos.retry_more();
        self.load_more_space_videos(cx);
    }

    fn set_space_order(&mut self, order: SpaceVideoOrder, cx: &mut Context<Self>) {
        if self.space_videos.order == order {
            return;
        }
        self.space_videos.order = order;
        self.load_space_videos(cx);
    }

    fn load_collections(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if !reset && (self.collections.loading || self.collections.loaded) {
            return;
        }
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.collections.error = Some(error);
                cx.notify();
                return;
            }
        };
        let generation = if reset {
            self.collections.reset()
        } else {
            self.collections.loading = true;
            self.collections.error = None;
            self.collections.generation
        };
        cx.notify();

        spawn_api(
            cx,
            async move { backend.collections(mid, 1).await },
            move |this, result, cx| {
                if this.collections.generation != generation {
                    return;
                }
                this.collections.loading = false;
                match result {
                    Ok(page) => {
                        this.collections.seasons = page
                            .items_lists
                            .seasons_list
                            .into_iter()
                            .map(|entry| entry.meta)
                            .collect();
                        this.collections.series = page
                            .items_lists
                            .series_list
                            .into_iter()
                            .map(|entry| entry.meta)
                            .collect();
                        this.collections.loaded = true;
                        this.collections.error = None;
                    }
                    Err(error) => {
                        this.collections.error = Some(api_task_error_message(error, cx));
                    }
                }
                cx.notify();
            },
        );
    }

    fn render_profile(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let _foreground = theme.foreground;
        let primary = theme.primary;
        let _primary_foreground = theme.primary_foreground;
        let _accent = theme.accent;
        let background = theme.background;
        let danger = theme.danger;
        let banner_bg = linear_gradient(
            120.,
            linear_color_stop(primary.opacity(0.32), 0.),
            linear_color_stop(background, 1.),
        );
        let radius = theme.radius_lg;

        let profile = self.profile.as_ref();
        let viewer = login_state::user(cx);
        let seed = viewer.as_ref().filter(|user| Some(user.mid) == self.mid);
        let name = profile
            .map(|profile| profile.user.name.as_str())
            .filter(|name| !name.is_empty())
            .or_else(|| seed.as_ref().map(|user| user.uname.as_str()))
            .unwrap_or(self.name.as_str());
        let face = profile
            .map(|profile| profile.user.face.clone())
            .or_else(|| seed.as_ref().map(|user| user.face.clone()))
            .unwrap_or_default();
        let sign = profile
            .map(|profile| profile.user.sign.clone())
            .unwrap_or_default();
        let banner_url = profile
            .map(|profile| profile.user.top_photo.as_str())
            .filter(|url| !url.is_empty());
        let level = profile
            .map(|profile| profile.user.level)
            .or_else(|| seed.as_ref().and_then(|user| user.level()))
            .unwrap_or(0);
        let official = profile
            .and_then(|profile| profile.official_title())
            .map(str::to_owned);
        let vip = profile
            .and_then(|profile| profile.vip_label())
            .map(str::to_owned)
            .or_else(|| {
                seed.as_ref()
                    .and_then(|user| user.vip_label())
                    .map(str::to_owned)
            });
        let stat_value = |value: Option<u64>| match value {
            Some(value) => format_count(value),
            None => "—".into(),
        };
        let following = stat_value(profile.and_then(|profile| profile.following()));
        let follower = stat_value(profile.and_then(|profile| profile.follower()));
        let likes = stat_value(profile.and_then(|profile| profile.likes()));
        let loading_sign = self.profile.is_none() && self.profile_loading;
        let error = self.profile_error.clone();
        let is_self = viewer
            .as_ref()
            .is_some_and(|user| Some(user.mid) == self.mid);
        let followed = self.followed;
        let follow_pending = self.follow_pending;
        let actions_ready = self.profile.is_some();

        v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(132.))
                    .rounded(radius)
                    .overflow_hidden()
                    .bg(banner_bg)
                    .when_some(banner_url, |this, url| {
                        this.child(
                            img(banner(url, 1280))
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .child(
                        div().absolute().top_3().left_3().child(
                            Button::new("space-back")
                                .secondary()
                                .rounded(ButtonRounded::Large)
                                .w(px(32.))
                                .h(px(32.))
                                .icon(IconName::ChevronLeft)
                                .accessibility_label("返回")
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(CloseSpace))),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex_shrink_0()
                            .mt(px(-32.))
                            .p_1()
                            .rounded_full()
                            .bg(background)
                            .child(Avatar::new().src(face).name(name.to_owned()).size(px(72.))),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .pt_2()
                            .child(profile_title(
                                name.to_owned(),
                                Some(level),
                                vip,
                                official,
                                cx,
                            ))
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "space-sign-{}",
                                        self.mid.unwrap_or_default()
                                    )))
                                    .h(px(18.))
                                    .truncate()
                                    .text_sm()
                                    .text_color(muted)
                                    .tooltip({
                                        let sign = sign.clone();
                                        move |window, cx| {
                                            Tooltip::new(SharedString::from(sign.clone()))
                                                .build(window, cx)
                                        }
                                    })
                                    .child(if !sign.is_empty() {
                                        sign.clone()
                                    } else if loading_sign {
                                        "正在加载…".to_owned()
                                    } else {
                                        "暂无简介".to_owned()
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_6()
                            .pt_3()
                            .child(
                                h_flex()
                                    .gap_8()
                                    .child(profile_stat("关注数", following, cx))
                                    .child(profile_stat("粉丝数", follower, cx))
                                    .child(profile_stat("获赞数", likes, cx)),
                            )
                            .when(!is_self, |this| {
                                this.child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Button::new("space-follow")
                                                .small()
                                                .min_w(px(84.))
                                                .when(followed, |button| {
                                                    button.secondary().icon(IconName::Check)
                                                })
                                                .when(!followed, |button| {
                                                    button.primary().icon(IconName::Plus)
                                                })
                                                .label(if followed {
                                                    "已关注"
                                                } else {
                                                    "关注"
                                                })
                                                .loading(follow_pending)
                                                .disabled(follow_pending || !actions_ready)
                                                .tooltip(if followed {
                                                    "取消关注"
                                                } else {
                                                    "关注 UP 主"
                                                })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.toggle_follow(window, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("space-message")
                                                .small()
                                                .primary()
                                                .outline()
                                                .label("发消息")
                                                .tooltip("发送私信")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.open_message(window, cx)
                                                })),
                                        ),
                                )
                            }),
                    ),
            )
            .when_some(error, |this, error| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(div().text_sm().text_color(danger).child(error))
                        .child(
                            Button::new("space-profile-retry")
                                .small()
                                .label("重试")
                                .on_click(cx.listener(|this, _, _, cx| this.load_profile(cx))),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_dynamics(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.dynamics.loading && self.dynamics.items.is_empty() {
            return centered_spinner();
        }
        if let Some(error) = self.dynamics.error.clone() {
            return error_state(
                Layout::Centered,
                error,
                "space-dynamics-retry",
                danger,
                cx.listener(|this, _, _, cx| this.reload_tab(cx)),
            );
        }
        if self.dynamics.items.is_empty() {
            return empty_state(Layout::Centered, muted, "暂无动态");
        }

        let needle = self.search.needle();
        let filtered =
            filter::filter_indices(&self.dynamics.items, needle.as_deref(), dynamic_matches);
        let cards = match &filtered {
            Some(indices) => indices
                .iter()
                .map(|&index| self.render_dynamic_card(&self.dynamics.items[index], index, cx))
                .collect::<Vec<_>>(),
            None => self
                .dynamics
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| self.render_dynamic_card(item, index, cx))
                .collect::<Vec<_>>(),
        };
        if cards.is_empty() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_3()
                .child(div().text_sm().text_color(muted).child("未找到匹配动态"))
                .child(self.render_dynamics_footer(cx))
                .into_any_element();
        }
        let footer = self.render_dynamics_footer(cx);

        crate::scroll::attach(
            div()
                .id("space-dynamics-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    h_flex().w_full().items_start().justify_center().child(
                        v_flex()
                            .w_full()
                            .max_w(DYNAMIC_CARD_MAX_WIDTH)
                            .gap_4()
                            .pb_4()
                            .children(cards)
                            .child(footer),
                    ),
                ),
            &self.dynamics_scroll,
            cx,
        )
        .into_any_element()
    }

    fn render_dynamics_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.dynamics.loading_more {
            h_flex()
                .h(LIST_FOOTER_HEIGHT)
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element()
        } else if let Some(error) = self.dynamics.more_error.clone() {
            list_footer(
                "space-dynamics-more",
                Some(error),
                muted,
                danger,
                cx.listener(|this, _, _, cx| this.retry_more_dynamics(cx)),
            )
        } else if self.dynamics.has_more {
            h_flex()
                .h(LIST_FOOTER_HEIGHT)
                .justify_center()
                .child(
                    Button::new("space-dynamics-more")
                        .small()
                        .ghost()
                        .label("加载更多")
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_dynamics(cx))),
                )
                .into_any_element()
        } else {
            div().h(LIST_FOOTER_HEIGHT).into_any_element()
        }
    }

    fn render_dynamic_card(
        &self,
        item: &DynamicItem,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let radius_lg = cx.theme().radius_lg;
        let card_bg = cx.theme().muted.opacity(0.25);
        let author = item.author();
        let text = item.text();
        let images = item.images();
        let archive = item.archive();
        let tip = item.forward_tip().map(str::to_owned);
        let forward = item.forward();
        let stat = item.stat();
        let comment_count = stat.map(|stat| stat.comment.count).unwrap_or(0);
        let like_count = stat.map(|stat| stat.like.count).unwrap_or(0);
        let key = comment_key(item);
        let expanded = self.expanded_comments.as_deref() == Some(key.as_str());
        let is_trigger = self.comment_trigger_key.as_deref() == Some(key.as_str());
        let comment_color = if expanded { primary } else { muted };
        let on_comments = {
            let dynamic = item.clone();
            let key = key.clone();
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.toggle_comments(key.clone(), dynamic.clone(), window, cx);
            })
        };

        v_flex()
            .id(("space-dynamic", index))
            .gap_3()
            .p_4()
            .rounded(radius_lg)
            .border_1()
            .border_color(border)
            .bg(card_bg)
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Avatar::new()
                            .src(author.face.clone())
                            .name(author.name.clone())
                            .size(px(36.)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(primary)
                                    .child(author.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(author.pub_time.clone()),
                            ),
                    )
                    .when(!author.pub_action.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(author.pub_action.clone()),
                        )
                    }),
            )
            .when(!text.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .line_height(relative(1.6))
                        .child(text.to_owned()),
                )
            })
            .when_some(images.first().copied(), |this, image| {
                this.child(dynamic_image(image, images.len(), radius))
            })
            .when_some(archive, |this, archive| {
                this.child(self.render_dynamic_archive(archive, index, cx))
            })
            .when_some(tip.clone(), |this, tip| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .p_3()
                        .rounded(radius)
                        .border_1()
                        .border_color(border)
                        .text_xs()
                        .text_color(muted)
                        .child(Icon::new(IconName::CircleAlert).with_size(px(16.)))
                        .child(tip),
                )
            })
            .when(forward.is_some() && tip.is_none(), |this| {
                if let Some(forward) = forward {
                    this.child(self.render_dynamic_forward(forward, index, cx))
                } else {
                    this
                }
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
                    .pt_3()
                    .text_sm()
                    .text_color(muted)
                    .child(action_item(IconName::Share, "转发".to_owned()))
                    .child(
                        div().flex_1().flex().justify_center().child(
                            Button::new(("space-dynamic-comments", index))
                                .text()
                                .small()
                                .px_2()
                                .when(is_trigger, |button| {
                                    button.track_focus(&self.comment_trigger_focus)
                                })
                                .accessibility_label(if expanded {
                                    "收起评论"
                                } else {
                                    "查看评论"
                                })
                                .tooltip(if expanded {
                                    "收起评论"
                                } else {
                                    "查看评论"
                                })
                                .on_click(on_comments)
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(comment_color)
                                        .child(
                                            Icon::new(IconName::MessageCircle).with_size(px(16.)),
                                        )
                                        .child(format_count(comment_count)),
                                ),
                        ),
                    )
                    .child(action_item(IconName::ThumbsUp, format_count(like_count))),
            )
            .when(expanded, |this| {
                this.child(
                    v_flex()
                        .id(("space-inline-comments", index))
                        .w_full()
                        .track_focus(&self.comments_focus)
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key.as_str() == "escape" {
                                this.collapse_comments(window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .gap_3()
                        .border_t_1()
                        .border_color(border)
                        .pt_4()
                        .child(
                            h_flex().w_full().justify_end().gap_2().child(
                                Button::new(("space-comments-collapse", index))
                                    .small()
                                    .ghost()
                                    .icon(IconName::ChevronUp)
                                    .label("收起")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.collapse_comments(window, cx)
                                    })),
                            ),
                        )
                        .when_some(self.comment_views.get(&key), |this, view| {
                            this.child(view.clone())
                        }),
                )
            })
            .into_any_element()
    }

    fn render_dynamic_archive(
        &self,
        archive: &DynamicArchive,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let open = OpenVideo {
            bvid: archive.bvid.clone(),
            cid: None,
            title: archive.title.clone(),
        };
        let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(open.clone()));

        h_flex()
            .id(("space-dynamic-archive", index))
            .items_center()
            .gap_3()
            .p_2()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .on_click(on_open)
            .child(
                div()
                    .w(px(160.))
                    .aspect_ratio(16. / 9.)
                    .rounded(radius)
                    .overflow_hidden()
                    .child(
                        img(thumbnail(&archive.cover, 320))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(div().text_sm().line_clamp(2).child(archive.title.clone()))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .text_xs()
                            .text_color(muted)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(IconName::Play)
                                    .child(archive.stat.play.clone()),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(IconName::MessageCircle)
                                    .child(archive.stat.danmaku.clone()),
                            )
                            .when(!archive.duration_text.is_empty(), |this| {
                                this.child(archive.duration_text.clone())
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_dynamic_forward(
        &self,
        forward: &DynamicItem,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let author = forward.author();
        let name = if author.name.is_empty() {
            "转发动态".to_owned()
        } else {
            author.name.clone()
        };

        v_flex()
            .id(("space-dynamic-forward", index))
            .gap_2()
            .p_3()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .child(div().text_xs().text_color(primary).child(name))
            .when(!forward.text().is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .line_height(relative(1.5))
                        .child(forward.text().to_owned()),
                )
            })
            .when_some(forward.archive(), |this, archive| {
                this.child(self.render_dynamic_archive(archive, index, cx))
            })
            .into_any_element()
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .child(
                h_flex().items_center().gap_2().children(
                    [
                        (SpaceVideoOrder::Latest, "最新发布", 0usize),
                        (SpaceVideoOrder::Plays, "最多播放", 1usize),
                    ]
                    .map(|(order, label, index)| {
                        let active = self.space_videos.order == order;
                        let group = SharedString::from(format!("space-order-{index}"));
                        v_flex()
                            .group(group.clone())
                            .items_center()
                            .rounded(radius)
                            .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                            .child(
                                Button::new(("space-order", index))
                                    .text()
                                    .small()
                                    .px_2()
                                    .accessibility_label(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_space_order(order, cx);
                                    }))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(if active { primary } else { muted })
                                            .group_hover(group, move |this| {
                                                this.text_color(primary)
                                            })
                                            .child(label),
                                    ),
                            )
                    }),
                ),
            )
            .when(self.space_videos.meta.total > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("共 {} 个投稿", self.space_videos.meta.total)),
                )
            })
            .into_any_element()
    }

    fn render_space_videos_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.space_videos.loading && self.space_videos.items.is_empty() {
            return centered_spinner();
        }
        if let Some(error) = self.space_videos.error.clone() {
            return error_state(
                Layout::Centered,
                error,
                "space-videos-retry",
                danger,
                cx.listener(|this, _, _, cx| this.reload_tab(cx)),
            );
        }
        if self.space_videos.items.is_empty() {
            return empty_state(
                Layout::Centered,
                muted,
                if self.search.is_empty() {
                    "暂无投稿"
                } else {
                    "未找到匹配投稿"
                },
            );
        }

        let style = CardStyle::new(cx);
        let columns = VideoCards::columns(self.list_width);
        let metrics = VideoCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        );
        let frame = CardFrame { style, metrics };
        let rows = self.space_videos.items.len().div_ceil(columns);
        let footer = self.space_videos.loading_more || self.space_videos.more_error.is_some();
        let sizes = Rows::new(rows, metrics.height()).footer_if(footer, LIST_FOOTER_HEIGHT);

        virtual_rows_tracked(
            cx,
            window,
            "space-video-list",
            sizes,
            self.list_width,
            &self.space_videos.scroll,
            move |this, range, window, cx| {
                this.maybe_load_more_space_videos(range.end.saturating_sub(1), rows, cx);
                range
                    .map(|row| {
                        if footer && row == rows {
                            this.render_space_videos_footer(cx)
                        } else {
                            this.render_space_video_row(row, columns, frame, window, cx)
                        }
                    })
                    .collect()
            },
        )
    }

    fn maybe_load_more_space_videos(
        &mut self,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows > 0 && last_visible + 1 >= rows {
            self.load_more_space_videos(cx);
        }
    }

    fn render_space_video_row(
        &self,
        row: usize,
        columns: usize,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        crate::grid::video_card_row(&self.space_videos.items, row, columns, frame, window, cx)
    }

    fn render_space_videos_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let more_error = self.space_videos.more_error.clone();

        list_footer(
            "space-videos-more",
            more_error,
            muted,
            danger,
            cx.listener(|this, _, _, cx| this.retry_more_space_videos(cx)),
        )
    }

    fn render_collections(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.collections.loading {
            return centered_spinner();
        }
        if let Some(error) = self.collections.error.clone() {
            return error_state(
                Layout::Centered,
                error,
                "space-collections-retry",
                danger,
                cx.listener(|this, _, _, cx| this.reload_tab(cx)),
            );
        }
        let needle = self.search.needle();
        let seasons = filter::matching(
            season_cards(&self.collections.seasons),
            needle.as_deref(),
            CollectionCard::matches,
        );
        let series = filter::matching(
            series_cards(&self.collections.series),
            needle.as_deref(),
            CollectionCard::matches,
        );
        if seasons.is_empty() && series.is_empty() {
            return collection_empty_state(
                muted,
                if needle.is_some() {
                    "未找到匹配合集或系列"
                } else {
                    "今天真是寂寞如雪啊~"
                },
            );
        }

        let columns = FolderCards::columns(self.list_width);
        let width = FolderCards::card_width(self.list_width, columns, window.scale_factor());
        let known_width = self.list_width;
        let weak = cx.entity().downgrade();
        let sections = [("合集", seasons), ("系列", series)]
            .into_iter()
            .filter(|(_, cards)| !cards.is_empty())
            .map(|(title, cards)| {
                render_collection_section(title, &cards, columns, width, muted, cx)
            })
            .collect::<Vec<_>>();

        crate::scroll::attach(
            div()
                .id("space-collections-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .on_prepaint(track_element_width(known_width, weak))
                .child(v_flex().gap_5().pb_4().children(sections)),
            &self.collections_scroll,
            cx,
        )
        .into_any_element()
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .child(
                TabStrip::new("space-tab")
                    .gap(px(16.))
                    .padding_x(px(8.))
                    .render(
                        cx,
                        &SpaceTab::ALL,
                        self.tab,
                        SpaceTab::label,
                        |_| None,
                        |this, tab, cx| this.show_tab(tab, cx),
                    ),
            )
            .child(div().flex_1())
            .child(self.search.field())
            .into_any_element()
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.tab {
            SpaceTab::Dynamics => self.render_dynamics(cx),
            SpaceTab::Videos => self.render_space_videos_content(window, cx),
            SpaceTab::Collections => self.render_collections(window, cx),
        }
    }
}

fn dynamic_matches(item: &DynamicItem, lowercase_needle: &str) -> bool {
    text_matches(item.text(), lowercase_needle)
        || text_matches(&item.author().name, lowercase_needle)
        || item
            .archive()
            .is_some_and(|archive| text_matches(&archive.title, lowercase_needle))
        || item
            .forward()
            .is_some_and(|forward| dynamic_matches(forward, lowercase_needle))
}

fn comment_key(item: &DynamicItem) -> String {
    let (kind, oid) = item.comment_target();
    format!("{kind}:{oid}")
}

impl Refresh for SpaceView {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load_profile(cx);
        self.reload_tab(cx);
    }

    fn can_refresh(&self) -> bool {
        self.mid.is_some()
    }
}

impl Render for SpaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.mid.is_none() {
            return div().size_full().into_any_element();
        }

        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let profile = self.render_profile(cx);
        let toolbar = (self.tab == SpaceTab::Videos).then(|| self.render_toolbar(cx));

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .child(profile)
            .child(self.render_tabs(cx))
            .when_some(toolbar, |this, toolbar| this.child(toolbar))
            .child(self.render_body(window, cx))
            .into_any_element()
    }
}

struct CollectionCard {
    name: String,
    cover: String,
    total: u64,
    kind: &'static str,
}

impl CollectionCard {
    fn matches(&self, needle: &str) -> bool {
        text_matches(&self.name, needle)
    }
}

fn season_cards(seasons: &[SeasonMeta]) -> Vec<CollectionCard> {
    seasons
        .iter()
        .map(|season| CollectionCard {
            name: season.name.clone(),
            cover: season.cover.clone(),
            total: season.total,
            kind: "合集",
        })
        .collect()
}

fn series_cards(series: &[SeriesMeta]) -> Vec<CollectionCard> {
    series
        .iter()
        .map(|series| CollectionCard {
            name: series.name.clone(),
            cover: series.cover.clone(),
            total: series.total,
            kind: "系列",
        })
        .collect()
}

fn space_page(
    page: rigili_bilibili::SpaceVideos,
) -> Result<Page<VideoCard, VideoMeta>, rigili_bilibili::ApiError> {
    let number = page.page.pn;
    let total = page.page.count;
    let incoming = page
        .list
        .vlist
        .into_iter()
        .map(VideoCard::from)
        .collect::<Vec<_>>();
    let has_more = !incoming.is_empty();
    Ok(Page::new(
        incoming,
        has_more,
        VideoMeta {
            page: number,
            total,
        },
    ))
}

fn space_videos_applied(this: &mut SpaceView) {
    let state = &mut this.space_videos;
    state.has_more = state.has_more && (state.items.len() as u64) < state.meta.total;
}

fn collection_empty_state(muted: Hsla, text: &'static str) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            Icon::new(IconName::ListVideo)
                .with_size(px(56.))
                .text_color(muted),
        )
        .child(div().text_sm().text_color(muted).child(text))
        .into_any_element()
}

fn render_collection_section(
    title: &'static str,
    cards: &[CollectionCard],
    columns: usize,
    width: Pixels,
    muted: Hsla,
    cx: &Context<SpaceView>,
) -> AnyElement {
    let rows = cards
        .chunks(columns)
        .map(|chunk| {
            h_flex()
                .gap(CARD_GAP)
                .children(chunk.iter().map(|card| collection_card(card, width, cx)))
                .into_any_element()
        })
        .collect::<Vec<_>>();

    v_flex()
        .gap_3()
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(cards.len().to_string()),
                ),
        )
        .child(v_flex().gap_y_6().children(rows))
        .into_any_element()
}

fn collection_card(card: &CollectionCard, width: Pixels, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let placeholder = theme.muted;
    let radius = theme.radius;

    v_flex()
        .w(width)
        .gap_2()
        .child(
            div()
                .w_full()
                .aspect_ratio(16. / 9.)
                .rounded(radius)
                .overflow_hidden()
                .bg(placeholder)
                .when(!card.cover.is_empty(), |this| {
                    this.child(
                        img(thumbnail(&card.cover, 480))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                }),
        )
        .child(div().text_sm().line_clamp(2).child(card.name.clone()))
        .child(div().text_xs().text_color(muted).child(format!(
            "{} · {} 个视频",
            card.kind,
            format_count(card.total)
        )))
        .into_any_element()
}

fn dynamic_image(image: &str, count: usize, radius: Pixels) -> AnyElement {
    div()
        .w_full()
        .aspect_ratio(16. / 9.)
        .relative()
        .rounded(radius)
        .overflow_hidden()
        .child(
            img(thumbnail(image, 720))
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .when(count > 1, |this| {
            this.child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .px_2()
                    .py_1()
                    .rounded(radius)
                    .bg(hsla(0., 0., 0., 0.55))
                    .text_xs()
                    .text_color(Hsla::white())
                    .child(format!("共 {count} 张")),
            )
        })
        .into_any_element()
}

fn action_item(icon: IconName, count: String) -> AnyElement {
    h_flex()
        .flex_1()
        .justify_center()
        .items_center()
        .gap_2()
        .child(icon)
        .child(count)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use rigili_bilibili::{
        DynamicArchive, DynamicAuthor, DynamicContent, DynamicDesc, DynamicItem, DynamicMajor,
        DynamicModules, SeasonMeta,
    };

    use super::{dynamic_matches, season_cards};
    use crate::filter;

    fn dynamic(text: &str, author: &str, archive: Option<&str>) -> DynamicItem {
        DynamicItem {
            modules: DynamicModules {
                module_author: DynamicAuthor {
                    name: author.into(),
                    ..Default::default()
                },
                module_dynamic: DynamicContent {
                    desc: Some(DynamicDesc { text: text.into() }),
                    major: archive.map(|title| DynamicMajor {
                        archive: Some(DynamicArchive {
                            title: title.into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn matches_dynamic_text_author_and_video_title() {
        let item = dynamic("今天聊聊 Rust", "小明", Some("GPUI 入门"));

        assert!(dynamic_matches(&item, "rust"));
        assert!(dynamic_matches(&item, "小明"));
        assert!(dynamic_matches(&item, "gpui"));
        assert!(!dynamic_matches(&item, "kotlin"));
    }

    #[test]
    fn matches_forwarded_dynamic() {
        let mut item = dynamic("转发", "UP主", None);
        item.orig = Some(Box::new(dynamic("", "", Some("被转发的视频"))));

        assert!(dynamic_matches(&item, "被转发"));
    }

    #[test]
    fn filters_collections_by_name() {
        let seasons = vec![
            SeasonMeta {
                name: "Rust 教程".into(),
                ..Default::default()
            },
            SeasonMeta {
                name: "旅行记录".into(),
                ..Default::default()
            },
        ];
        let cards = season_cards(&seasons);
        assert_eq!(cards.len(), 2);

        let cards = filter::matching(cards, Some("rust"), super::CollectionCard::matches);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name, "Rust 教程");
    }
}
