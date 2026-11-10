gpuili_macro::mod_prv!(feed, list);

use gpui_kit::base::Selectable;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::api::search::SearchOrder;

use crate::backend::{self, Backend};
use crate::format::format_count;
use crate::grid::{OpenVideo, VideoCard};

pub struct SearchView {
    backend: Option<Backend>,
    keyword: String,
    order: SearchOrder,
    videos: Vec<VideoCard>,
    page: u32,
    num_pages: u32,
    num_results: u64,
    loading: bool,
    loading_more: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
    list_width: Pixels,
    scroll: VirtualListScrollHandle,
}

impl EventEmitter<OpenVideo> for SearchView {}

impl SearchView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (backend, error) = match backend::get(cx) {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error)),
        };

        Self {
            backend,
            keyword: String::new(),
            order: SearchOrder::Default,
            videos: Vec::new(),
            page: 0,
            num_pages: 0,
            num_results: 0,
            loading: false,
            loading_more: false,
            error,
            more_error: None,
            generation: 0,
            list_width: px(0.),
            scroll: VirtualListScrollHandle::new(),
        }
    }

    pub(crate) fn search(&mut self, keyword: String, cx: &mut Context<Self>) {
        self.keyword = keyword;
        self.load(cx);
    }

    pub(crate) fn set_order(&mut self, order: SearchOrder, cx: &mut Context<Self>) {
        if self.order == order {
            return;
        }
        self.order = order;
        self.load(cx);
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let show_count = !self.keyword.is_empty() && !self.loading && self.error.is_none();

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .items_end()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(primary)
                                    .child("视频"),
                            )
                            .child(div().h(px(2.)).rounded_full().bg(primary)),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(h_flex().items_center().gap_1().children(
                        SearchOrder::ALL.into_iter().map(|order| {
                            let active = self.order == order;
                            Button::new(order.as_str())
                                .ghost()
                                .small()
                                .selected(active)
                                .when(active, |this| this.text_color(primary))
                                .label(order_label(order))
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.set_order(order, cx)),
                                )
                        }),
                    ))
                    .when(show_count, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("共 {} 个视频", format_count(self.num_results))),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn order_label(order: SearchOrder) -> &'static str {
    match order {
        SearchOrder::Default => "综合排序",
        SearchOrder::Clicks => "最多点击",
        SearchOrder::Newest => "最新发布",
        SearchOrder::Danmaku => "最多弹幕",
        SearchOrder::Favorites => "最多收藏",
    }
}

impl Render for SearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let content = self.render_content(window, cx);

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_4()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .child(self.render_toolbar(cx))
            .child(content)
    }
}
