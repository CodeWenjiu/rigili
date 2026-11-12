use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::cards::{
    ROW_HEIGHT, article_row, live_card, media_item_card, user_block_height, user_row,
};
use super::items::SearchItem;
use super::{SearchLayout, SearchTab, SearchView};
use crate::grid::{
    CARD_GAP, CardFrame, CardGrid, CardMetrics, CardStyle, LIST_FOOTER_HEIGHT, VideoCardKind,
    VideoCards, card_text_height, list_content_size, list_footer, page_gutter, skeleton_card,
    video_card, virtual_list_scrollbar,
};
use rigili_core::{OpenLive, OpenSpace};

impl SearchView {
    fn columns(&self) -> usize {
        VideoCards::columns(self.list_width)
    }

    fn card_metrics(&self, window: &Window) -> CardMetrics {
        VideoCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        )
    }

    fn render_grid_row(
        &self,
        tab: SearchTab,
        row: usize,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let CardFrame { style, metrics } = frame;
        let state = self.state(tab);
        let columns = self.columns().max(1);
        let start = row * columns;
        let end = (start + columns).min(state.items.len());

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| match &state.items[index] {
                SearchItem::Video(video) => {
                    let open = video.open();
                    let on_open =
                        cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(open.clone()));
                    video_card(
                        video,
                        VideoCardKind::Search,
                        style,
                        metrics,
                        window,
                        cx,
                        on_open,
                    )
                }
                SearchItem::Media(media) => {
                    let season_id = media.season_id;
                    let on_open = cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_season(season_id, cx)
                    });
                    media_item_card(media, style, metrics, window, cx, on_open)
                }
                SearchItem::Live(live) => {
                    let room_id = live.room_id;
                    let title = live.title.clone();
                    let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(OpenLive {
                            room_id,
                            title: title.clone(),
                        });
                    });
                    live_card(live, style, metrics, window, cx, on_open)
                }
                SearchItem::Article(_) | SearchItem::User(_) => div().into_any_element(),
            }))
            .into_any_element()
    }

    fn render_row_item(
        &self,
        item: &SearchItem,
        style: CardStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match item {
            SearchItem::Article(article) => {
                let id = article.id;
                let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.open_url(&format!("https://www.bilibili.com/read/cv{id}"));
                });
                article_row(article, style, on_open)
            }
            SearchItem::User(user) => {
                let mid = user.mid;
                let name = user.name.clone();
                let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(OpenSpace {
                        mid,
                        name: name.clone(),
                    });
                });
                user_row(user, style, on_open)
            }
            _ => div().into_any_element(),
        }
    }

    fn render_user_block_at(
        &self,
        tab: SearchTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.state(tab).users.first() {
            Some(user) => self.render_user_block(user, window, cx),
            None => div().into_any_element(),
        }
    }

    fn render_item_at(
        &self,
        tab: SearchTab,
        row: usize,
        layout: SearchLayout,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if layout == SearchLayout::Grid {
            return self.render_grid_row(tab, row, frame, window, cx);
        }

        match self.state(tab).items.get(row) {
            Some(item) => self.render_row_item(item, frame.style, cx),
            None => div().into_any_element(),
        }
    }

    fn render_footer(&self, tab: SearchTab, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let more_error = self.state(tab).more_error.clone();

        list_footer(
            ("search-retry-more", tab.index()),
            more_error,
            muted,
            danger,
            cx.listener(move |this, _, _, cx| this.retry_more(tab, cx)),
        )
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.tab;
        let style = CardStyle::new(cx);
        let gutter = page_gutter(window);
        let metrics = self.card_metrics(window);
        let frame = CardFrame { style, metrics };
        let columns = self.columns().max(1);
        let layout = tab.layout();
        let has_header = tab == SearchTab::All && !self.state(tab).users.is_empty();
        let item_count = self.state(tab).items.len();
        let rows = match layout {
            SearchLayout::Grid => item_count.div_ceil(columns),
            SearchLayout::Rows => item_count,
        };
        let header_offset = usize::from(has_header);
        let footer = self.state(tab).loading_more || self.state(tab).more_error.is_some();
        let footer_index = header_offset + rows;
        let header_height = user_block_height(self.user_content_width());
        let row_height = match layout {
            SearchLayout::Grid => metrics.height(),
            SearchLayout::Rows => ROW_HEIGHT,
        };
        let known_width = self.list_width;

        let mut sizes = Vec::with_capacity(footer_index + usize::from(footer));
        if has_header {
            sizes.push(size(px(0.), header_height));
        }
        sizes.extend(std::iter::repeat_n(size(px(0.), row_height), rows));
        if footer {
            sizes.push(size(px(0.), LIST_FOOTER_HEIGHT));
        }
        let content_size = list_content_size(&sizes, rems(1.5).to_pixels(window.rem_size()));
        let sizes = Rc::new(sizes);

        let view = cx.entity();
        let weak = view.downgrade();
        let list = v_virtual_list(
            view,
            tab.list_id(),
            sizes,
            move |this, visible_range, window, cx| {
                this.maybe_load_more(
                    tab,
                    visible_range.end.saturating_sub(1),
                    rows,
                    header_offset,
                    cx,
                );
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_footer(tab, cx)
                        } else if has_header && row == 0 {
                            this.render_user_block_at(tab, window, cx)
                        } else {
                            this.render_item_at(tab, row - header_offset, layout, frame, window, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            },
        )
        .gap_y_6()
        .track_scroll(&self.state(tab).scroll);

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .mr(px(-gutter.as_f32()))
            .pr(gutter)
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
            .child(virtual_list_scrollbar(
                &self.state(tab).scroll,
                content_size,
            ))
            .into_any_element()
    }

    pub(super) fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab = self.tab;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let loading = self.state(tab).loading;
        let error = self.state(tab).error.clone();
        let empty = self.state(tab).items.is_empty()
            && (tab != SearchTab::All || self.state(tab).users.is_empty());

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
                        .child("按回车或点击搜索，查找哔哩哔哩的视频、番剧和用户"),
                )
                .into_any_element();
        }

        if loading {
            let columns = self.columns();
            if tab.layout() == SearchLayout::Rows {
                return v_flex()
                    .id("search-loading")
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .children(
                        (0..5).map(|_| Skeleton::new().h(ROW_HEIGHT).w_full().rounded(radius)),
                    )
                    .into_any_element();
            }

            let metrics = VideoCards::metrics(
                self.list_width,
                card_text_height(window),
                window.scale_factor(),
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

        if let Some(error) = error {
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
                        .on_click(cx.listener(move |this, _, _, cx| this.load_tab(tab, cx))),
                )
                .into_any_element();
        }

        if empty {
            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_2()
                .child(div().text_sm().text_color(foreground).child(format!(
                    "没有找到与“{}”相关的{}",
                    self.keyword,
                    tab.empty_label()
                )))
                .child(div().text_xs().text_color(muted).child("换个关键词试试"))
                .into_any_element();
        }

        self.render_list(window, cx)
    }
}
