use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::space::SpaceVideoOrder;
use rigili_bilibili::{DynamicArchive, DynamicItem, SeasonMeta, SeriesMeta, thumbnail};
use rigili_core::runtime::spawn_api;
use rigili_core::{OpenVideo, api_task_error_message, format_count};

use super::list::video_card;
use super::{MineTab, MineView};
use crate::grid::{
    CARD_GAP, CardGrid, CardMetrics, CardStyle, FolderCards, LIST_FOOTER_HEIGHT, VideoCard,
    VideoCards, append_unique_by, card_text_height, list_footer,
};

const DYNAMIC_CARD_MAX_WIDTH: Pixels = px(840.);

pub(super) struct DynamicsState {
    items: Vec<DynamicItem>,
    offset: String,
    has_more: bool,
    loading: bool,
    loading_more: bool,
    loaded: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
}

impl DynamicsState {
    pub(super) fn new() -> Self {
        Self {
            items: Vec::new(),
            offset: String::new(),
            has_more: false,
            loading: false,
            loading_more: false,
            loaded: false,
            error: None,
            more_error: None,
            generation: 0,
        }
    }

    fn reset(&mut self) -> u64 {
        self.generation += 1;
        self.items.clear();
        self.offset.clear();
        self.has_more = true;
        self.loading = true;
        self.loading_more = false;
        self.loaded = false;
        self.error = None;
        self.more_error = None;
        self.generation
    }
}

pub(super) struct SpaceVideosState {
    items: Vec<VideoCard>,
    page: u32,
    total: u64,
    keyword: String,
    order: SpaceVideoOrder,
    has_more: bool,
    loading: bool,
    loading_more: bool,
    loaded: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
    scroll: VirtualListScrollHandle,
}

impl SpaceVideosState {
    pub(super) fn new() -> Self {
        Self {
            items: Vec::new(),
            page: 0,
            total: 0,
            keyword: String::new(),
            order: SpaceVideoOrder::default(),
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
        self.total = 0;
        self.has_more = true;
        self.loading = true;
        self.loading_more = false;
        self.loaded = false;
        self.error = None;
        self.more_error = None;
        self.generation
    }
}

pub(super) struct CollectionsState {
    seasons: Vec<SeasonMeta>,
    series: Vec<SeriesMeta>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
}

impl CollectionsState {
    pub(super) fn new() -> Self {
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

    fn is_empty(&self) -> bool {
        self.seasons.is_empty() && self.series.is_empty()
    }
}

impl MineView {
    pub(super) fn load_space_if_needed(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            MineTab::Dynamics
                if !self.dynamics.loaded
                    && !self.dynamics.loading
                    && self.dynamics.error.is_none() =>
            {
                self.load_dynamics(true, cx);
            }
            MineTab::Videos
                if !self.space_videos.loaded
                    && !self.space_videos.loading
                    && self.space_videos.error.is_none() =>
            {
                self.load_space_videos(true, cx);
            }
            MineTab::Collections
                if !self.collections.loaded
                    && !self.collections.loading
                    && self.collections.error.is_none() =>
            {
                self.load_collections(true, cx);
            }
            _ => {}
        }
    }

    pub(super) fn reload_space_tab(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            MineTab::Dynamics => self.load_dynamics(true, cx),
            MineTab::Videos => self.load_space_videos(true, cx),
            MineTab::Collections => self.load_collections(true, cx),
            _ => {}
        }
    }

    fn load_dynamics(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
            return;
        };
        if !reset && (self.dynamics.loading || self.dynamics.loading_more) {
            return;
        }
        if !reset && (!self.dynamics.has_more || self.dynamics.offset.is_empty()) {
            return;
        }
        let offset = if reset {
            String::new()
        } else {
            self.dynamics.offset.clone()
        };
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.dynamics.error = Some(error);
                cx.notify();
                return;
            }
        };
        let generation = if reset {
            self.dynamics.reset()
        } else {
            self.dynamics.loading_more = true;
            self.dynamics.more_error = None;
            self.dynamics.generation
        };
        cx.notify();

        spawn_api(
            cx,
            async move { backend.dynamics(mid, &offset).await },
            move |this, result, cx| {
                if this.dynamics.generation != generation {
                    return;
                }
                this.dynamics.loading = false;
                this.dynamics.loading_more = false;
                match result {
                    Ok(page) => {
                        append_unique_by(&mut this.dynamics.items, page.items, |item| {
                            item.id_str.clone()
                        });
                        this.dynamics.offset = page.offset;
                        this.dynamics.has_more = page.has_more;
                        this.dynamics.loaded = true;
                        this.dynamics.error = None;
                        this.dynamics.more_error = None;
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        if reset {
                            this.dynamics.error = Some(message);
                        } else {
                            this.dynamics.more_error = Some(message);
                        }
                    }
                }
                cx.notify();
            },
        );
    }

    fn load_more_dynamics(&mut self, cx: &mut Context<Self>) {
        self.load_dynamics(false, cx);
    }

    fn load_space_videos(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
            return;
        };
        if !reset
            && (self.space_videos.loading
                || self.space_videos.loading_more
                || !self.space_videos.has_more)
        {
            return;
        }
        let page = if reset { 1 } else { self.space_videos.page + 1 };
        let order = self.space_videos.order;
        let keyword = self.space_videos.keyword.clone();
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.space_videos.error = Some(error);
                cx.notify();
                return;
            }
        };
        let generation = if reset {
            self.space_videos.reset()
        } else {
            self.space_videos.loading_more = true;
            self.space_videos.more_error = None;
            self.space_videos.generation
        };
        cx.notify();

        spawn_api(
            cx,
            async move { backend.space_videos(mid, page, order, &keyword).await },
            move |this, result, cx| {
                if this.space_videos.generation != generation {
                    return;
                }
                this.space_videos.loading = false;
                this.space_videos.loading_more = false;
                match result {
                    Ok(page) => {
                        let incoming = page
                            .list
                            .vlist
                            .into_iter()
                            .map(VideoCard::from)
                            .collect::<Vec<_>>();
                        let incoming_empty = incoming.is_empty();
                        append_unique_by(&mut this.space_videos.items, incoming, |card| {
                            card.bvid().to_owned()
                        });
                        this.space_videos.page = page.page.pn;
                        this.space_videos.total = page.page.count;
                        this.space_videos.has_more = !incoming_empty
                            && (this.space_videos.items.len() as u64) < page.page.count;
                        this.space_videos.loaded = true;
                        this.space_videos.error = None;
                        this.space_videos.more_error = None;
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        if reset {
                            this.space_videos.error = Some(message);
                        } else {
                            this.space_videos.more_error = Some(message);
                        }
                    }
                }
                cx.notify();
            },
        );
    }

    fn load_more_space_videos(&mut self, cx: &mut Context<Self>) {
        self.load_space_videos(false, cx);
    }

    fn set_space_order(&mut self, order: SpaceVideoOrder, cx: &mut Context<Self>) {
        if self.space_videos.order == order {
            return;
        }
        self.space_videos.order = order;
        self.load_space_videos(true, cx);
    }

    pub(super) fn submit_space_search(&mut self, cx: &mut Context<Self>) {
        let keyword = self.space_search.read(cx).value().trim().to_owned();
        if keyword == self.space_videos.keyword {
            return;
        }
        self.space_videos.keyword = keyword;
        self.load_space_videos(true, cx);
    }

    fn load_collections(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.user_mid else {
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

    pub(super) fn render_dynamics(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.dynamics.loading && self.dynamics.items.is_empty() {
            return centered_spinner();
        }
        if let Some(error) = self.dynamics.error.clone() {
            return error_state(error, danger, "mine-dynamics-retry", cx);
        }
        if self.dynamics.items.is_empty() {
            return empty_state(muted, "暂无动态");
        }

        let cards = self
            .dynamics
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| self.render_dynamic_card(item, index, cx))
            .collect::<Vec<_>>();
        let footer = if self.dynamics.loading_more {
            h_flex()
                .h(LIST_FOOTER_HEIGHT)
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element()
        } else if let Some(error) = self.dynamics.more_error.clone() {
            list_footer(
                "mine-dynamics-more",
                Some(error),
                muted,
                danger,
                cx.listener(|this, _, _, cx| this.load_more_dynamics(cx)),
            )
        } else if self.dynamics.has_more {
            h_flex()
                .h(LIST_FOOTER_HEIGHT)
                .justify_center()
                .child(
                    Button::new("mine-dynamics-more")
                        .small()
                        .ghost()
                        .label("加载更多")
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_dynamics(cx))),
                )
                .into_any_element()
        } else {
            div().h(LIST_FOOTER_HEIGHT).into_any_element()
        };

        div()
            .id("mine-dynamics-scroll")
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
            )
            .into_any_element()
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

        v_flex()
            .id(("mine-dynamic", index))
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
                    .child(action_item(
                        IconName::MessageCircle,
                        format_count(comment_count),
                    ))
                    .child(action_item(IconName::ThumbsUp, format_count(like_count))),
            )
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
            .id(("mine-dynamic-archive", index))
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
            .id(("mine-dynamic-forward", index))
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

    pub(super) fn render_space_videos(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let toolbar = self.render_space_videos_toolbar(cx);
        let content = self.render_space_videos_content(window, cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .child(toolbar)
            .child(content)
            .into_any_element()
    }

    fn render_space_videos_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
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
                        let group = SharedString::from(format!("mine-space-order-{index}"));
                        v_flex()
                            .group(group.clone())
                            .items_center()
                            .rounded(radius)
                            .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                            .child(
                                Button::new(("mine-space-order", index))
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
            .child(div().flex_1())
            .child(
                div().w(px(220.)).child(
                    Input::new(&self.space_search)
                        .small()
                        .cleanable(true)
                        .prefix(IconName::Search),
                ),
            )
            .when(self.space_videos.total > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("共 {} 个投稿", self.space_videos.total)),
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
            return error_state(error, danger, "mine-space-videos-retry", cx);
        }
        if self.space_videos.items.is_empty() {
            return empty_state(muted, "暂无投稿");
        }

        let style = CardStyle::new(cx);
        let columns = VideoCards::columns(self.list_width);
        let metrics = VideoCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        );
        let row_height = metrics.height();
        let rows = self.space_videos.items.len().div_ceil(columns);
        let known_width = self.list_width;
        let footer = self.space_videos.loading_more || self.space_videos.more_error.is_some();
        let footer_index = rows;

        let mut sizes = vec![size(px(0.), row_height); rows];
        if footer {
            sizes.push(size(px(0.), LIST_FOOTER_HEIGHT));
        }
        let sizes = Rc::new(sizes);

        let view = cx.entity();
        let weak = view.downgrade();
        let list = v_virtual_list(
            view,
            "mine-space-video-list",
            sizes,
            move |this, visible_range, _, cx| {
                this.maybe_load_more_space_videos(visible_range.end.saturating_sub(1), rows, cx);
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_space_videos_footer(cx)
                        } else {
                            this.render_space_video_row(row, columns, metrics, style, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            },
        )
        .gap_y_6()
        .track_scroll(&self.space_videos.scroll);

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_children_prepainted(move |bounds, window, cx| {
                let Some(width) = bounds.first().map(|bounds| bounds.size.width) else {
                    return;
                };
                if width == known_width {
                    return;
                }
                let weak = weak.clone();
                window.defer(cx, move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.list_width != width {
                            this.list_width = width;
                            cx.notify();
                        }
                    });
                });
            })
            .child(list)
            .child(Scrollbar::vertical(&self.space_videos.scroll).viewport_from_layout())
            .into_any_element()
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
        metrics: CardMetrics,
        style: CardStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let start = row * columns;
        let end = (start + columns).min(self.space_videos.items.len());

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| {
                let card = &self.space_videos.items[index];
                let open = card.open();
                let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(open.clone());
                });
                video_card(card, style, metrics, on_open)
            }))
            .into_any_element()
    }

    fn render_space_videos_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let more_error = self.space_videos.more_error.clone();

        list_footer(
            "mine-space-videos-more",
            more_error,
            muted,
            danger,
            cx.listener(|this, _, _, cx| this.load_more_space_videos(cx)),
        )
    }

    pub(super) fn render_collections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.collections.loading {
            return centered_spinner();
        }
        if let Some(error) = self.collections.error.clone() {
            return error_state(error, danger, "mine-collections-retry", cx);
        }
        if self.collections.is_empty() {
            return v_flex()
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
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("今天真是寂寞如雪啊~"),
                )
                .into_any_element();
        }

        let columns = FolderCards::columns(self.list_width);
        let width = FolderCards::card_width(self.list_width, columns, window.scale_factor());
        let known_width = self.list_width;
        let weak = cx.entity().downgrade();
        let sections = [
            ("合集", season_cards(&self.collections.seasons)),
            ("系列", series_cards(&self.collections.series)),
        ]
        .into_iter()
        .filter(|(_, cards)| !cards.is_empty())
        .map(|(title, cards)| render_collection_section(title, &cards, columns, width, muted, cx))
        .collect::<Vec<_>>();

        div()
            .id("mine-collections-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .on_prepaint(move |bounds, window, cx| {
                let width = bounds.size.width;
                if width == known_width {
                    return;
                }
                let weak = weak.clone();
                window.defer(cx, move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.list_width != width {
                            this.list_width = width;
                            cx.notify();
                        }
                    });
                });
            })
            .child(v_flex().gap_5().pb_4().children(sections))
            .into_any_element()
    }
}

struct CollectionCard {
    name: String,
    cover: String,
    total: u64,
    kind: &'static str,
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

fn render_collection_section(
    title: &'static str,
    cards: &[CollectionCard],
    columns: usize,
    width: Pixels,
    muted: Hsla,
    cx: &Context<MineView>,
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

fn centered_spinner() -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .child(Spinner::new())
        .into_any_element()
}

fn error_state(
    error: String,
    danger: Hsla,
    id: &'static str,
    cx: &mut Context<MineView>,
) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .gap_3()
        .child(div().text_sm().text_color(danger).child(error))
        .child(
            Button::new(id)
                .label("重试")
                .on_click(cx.listener(|this, _, _, cx| this.reload_space_tab(cx))),
        )
        .into_any_element()
}

fn empty_state(muted: Hsla, text: &'static str) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .child(div().text_sm().text_color(muted).child(text))
        .into_any_element()
}
