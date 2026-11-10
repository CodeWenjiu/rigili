use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SearchView;
use crate::grid::{
    CARD_GAP, CardMetrics, CardStyle, card_text_height, card_width_for_width, columns_for_width,
    skeleton_card, video_card,
};

const FOOTER_HEIGHT: Pixels = px(56.);

impl SearchView {
    fn columns(&self) -> usize {
        columns_for_width(self.list_width)
    }

    fn render_row(
        &self,
        row: usize,
        columns: usize,
        metrics: CardMetrics,
        style: CardStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let start = row * columns;
        let end = (start + columns).min(self.videos.len());

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| {
                let video = &self.videos[index];
                let open = video.open();
                let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(open.clone());
                });
                video_card(video, style, metrics.width, metrics.text_height, on_open)
            }))
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let content = if let Some(error) = self.more_error.clone() {
            h_flex()
                .items_center()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("search-retry-more")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.retry_more(cx))),
                )
                .into_any_element()
        } else {
            h_flex()
                .items_center()
                .gap_2()
                .child(Spinner::new().small())
                .child(div().text_sm().text_color(muted).child("正在加载…"))
                .into_any_element()
        };

        h_flex()
            .h(FOOTER_HEIGHT)
            .items_center()
            .justify_center()
            .child(content)
            .into_any_element()
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let style = CardStyle::new(cx);
        let columns = self.columns();
        let metrics = CardMetrics::new(
            card_width_for_width(self.list_width, columns),
            card_text_height(window),
        );
        let row_height = metrics.height();
        let rows = self.videos.len().div_ceil(columns);
        let known_width = self.list_width;
        let footer = self.loading_more || self.more_error.is_some();
        let footer_index = rows;

        let mut sizes = vec![size(px(0.), row_height); rows];
        if footer {
            sizes.push(size(px(0.), FOOTER_HEIGHT));
        }
        let sizes = Rc::new(sizes);

        let view = cx.entity();
        let weak = view.downgrade();
        let list = v_virtual_list(
            view,
            "search-list",
            sizes,
            move |this, visible_range, _, cx| {
                this.maybe_load_more(visible_range.end.saturating_sub(1), rows, cx);
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_footer(cx)
                        } else {
                            this.render_row(row, columns, metrics, style, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            },
        )
        .gap_y_6()
        .track_scroll(&self.scroll);

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
                // Notifying during prepaint does not schedule another frame.
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
            .child(Scrollbar::vertical(&self.scroll).viewport_from_layout())
            .into_any_element()
    }

    pub(super) fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;

        if self.loading {
            let columns = self.columns();
            let metrics = CardMetrics::new(
                card_width_for_width(self.list_width, columns),
                card_text_height(window),
            );
            return div()
                .id("search-loading")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_x(CARD_GAP)
                        .gap_y_6()
                        .children((0..columns * 2).map(|_| skeleton_card(metrics.width, radius))),
                )
                .into_any_element();
        }

        if let Some(error) = self.error.clone() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("search-retry")
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                )
                .into_any_element();
        }

        if self.videos.is_empty() {
            if self.keyword.is_empty() {
                return v_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(Icon::new(IconName::Search).large().text_color(muted))
                    .child(
                        div()
                            .text_sm()
                            .text_color(foreground)
                            .child("输入关键词开始搜索"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("按回车或点击搜索，查找哔哩哔哩视频"),
                    )
                    .into_any_element();
            }

            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(foreground)
                        .child(format!("没有找到与“{}”相关的视频", self.keyword)),
                )
                .child(div().text_xs().text_color(muted).child("换个关键词试试"))
                .into_any_element();
        }

        self.render_list(window, cx)
    }
}
