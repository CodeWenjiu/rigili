rigili_macro::mod_prv!(collections, dynamics, profile, videos);

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::space::SpaceVideoOrder;
use rigili_bilibili::{Client, DynamicItem, SeasonMeta, SeriesMeta, SpaceProfile};

use rigili_core::client;
use rigili_core::{OpenSpace, OpenVideo, Refresh};

use super::comment;
use crate::shared::browse::filter;
use crate::shared::browse::grid::VideoCard;
use crate::shared::browse::paging::Paged;
use crate::shared::ui::scroll;
use crate::shared::ui::tabs::TabStrip;

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
    client: Option<Arc<Client>>,
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
    space_videos: Paged<VideoCard, VideoMeta>,
    space_order: SpaceVideoOrder,
    collections: CollectionsState,
    dynamics_scroll: scroll::ScrollArea,
    collections_scroll: scroll::ScrollArea,
    list_width: Pixels,
    search: filter::Search,
}

impl EventEmitter<OpenVideo> for SpaceView {}
impl EventEmitter<OpenSpace> for SpaceView {}
impl EventEmitter<CloseSpace> for SpaceView {}

impl crate::shared::browse::list::ListWidth for SpaceView {
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
            client: None,
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
            space_videos: Paged::new(),
            space_order: SpaceVideoOrder::default(),
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
        self.space_videos = Paged::new();
        self.space_order = SpaceVideoOrder::default();
        self.collections = CollectionsState::new();
        self.list_width = px(0.);
        self.search.clear(window, cx);
        self.load_profile(cx);
        self.load_if_needed(cx);
        cx.notify();
    }

    fn cached_client(&mut self, cx: &mut Context<Self>) -> Result<Arc<Client>, String> {
        client::cached(&mut self.client, cx)
    }

    fn load_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.mid.is_none() {
            return;
        }
        match self.tab {
            SpaceTab::Dynamics if self.dynamics.needs_first_load() => {
                self.load_dynamics(cx);
            }
            SpaceTab::Videos if self.space_videos.needs_first_load() => {
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
            self.space_videos = Paged::new();
            self.space_order = SpaceVideoOrder::default();
            self.load_if_needed(cx);
            cx.notify();
        }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VideoMeta {
    pub(crate) page: u32,
    pub(crate) total: u64,
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
