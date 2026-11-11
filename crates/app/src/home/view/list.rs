use std::rc::Rc;

use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{HomeItem, HomeTab, HomeView};
use crate::grid::{
    CARD_GAP, CardGrid, CardMetrics, CardOverlay, CardStyle, LIST_FOOTER_HEIGHT, MediaAuthor,
    MediaCard, OpenHandler, VideoCards, card_text_height, list_footer, media_card, skeleton_card,
};
use rigili_core::OpenSpace;

impl HomeView {
    fn render_row(
        &self,
        tab: HomeTab,
        row: usize,
        columns: usize,
        metrics: CardMetrics,
        style: CardStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(tab);
        let start = row * columns;
        let end = (start + columns).min(state.items.len());
        let opening = self.opening;

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| {
                let item = &state.items[index];
                let target = item.target();
                let is_opening = item
                    .season_id()
                    .is_some_and(|season_id| opening == Some(season_id));
                let on_open = cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_target(target.clone(), cx);
                });
                let on_author = item.author_mid().map(|mid| {
                    let name = item.author().to_owned();
                    let handler: Rc<OpenHandler> =
                        Rc::new(cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.emit(OpenSpace {
                                mid,
                                name: name.clone(),
                            });
                        }));
                    MediaAuthor { on_click: handler }
                });
                home_card(item, is_opening, style, metrics, on_open, on_author)
            }))
            .into_any_element()
    }

    fn render_footer(&self, tab: HomeTab, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let more_error = self.state(tab).more_error.clone();

        list_footer(
            ("home-retry-more", tab.index()),
            more_error,
            muted,
            danger,
            cx.listener(move |this, _, _, cx| this.retry_more(tab, cx)),
        )
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.tab;
        let style = CardStyle::new(cx);
        let state = self.state(tab);
        let columns = VideoCards::columns(self.list_width);
        let metrics = VideoCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        );
        let row_height = metrics.height();
        let rows = state.items.len().div_ceil(columns);
        let known_width = self.list_width;
        let footer = state.loading_more || state.more_error.is_some();
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
            tab.list_id(),
            sizes,
            move |this, visible_range, _, cx| {
                this.maybe_load_more(tab, visible_range.end.saturating_sub(1), rows, cx);
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(tab, row, columns, metrics, style, cx)
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
            .child(Scrollbar::vertical(&self.state(tab).scroll).viewport_from_layout())
            .into_any_element()
    }

    pub(super) fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab = self.tab;
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let state = self.state(tab);
        let loading = state.loading;
        let error = state.error.clone();
        let empty = state.items.is_empty();

        if loading {
            let columns = VideoCards::columns(self.list_width);
            let metrics = VideoCards::metrics(
                self.list_width,
                card_text_height(window),
                window.scale_factor(),
            );
            return div()
                .id("loading")
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
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("retry")
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.reload_current(cx))),
                )
                .into_any_element();
        }

        if empty {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("暂无内容")
                .into_any_element();
        }

        self.render_list(window, cx)
    }
}

fn home_card(
    item: &HomeItem,
    opening: bool,
    style: CardStyle,
    metrics: CardMetrics,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_author: Option<MediaAuthor>,
) -> AnyElement {
    let card = media_card(
        MediaCard {
            id: item.id().clone(),
            cover: item.cover(),
            title: item.title(),
            footer: Some(item.footer()),
            footer_icon: None,
            title_lines: 2,
            thumb_width: metrics.thumb_width,
            overlay: CardOverlay {
                leading: item.leading(),
                secondary: item.secondary(),
                trailing: item.trailing(),
                badge: item.badge(),
            },
            author: on_author,
        },
        style,
        metrics.width,
        metrics.text_height,
        on_open,
    );

    if !opening {
        return card.into_any_element();
    }

    div()
        .relative()
        .w(metrics.width)
        .h(metrics.height())
        .child(card)
        .child(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0., 0.45))
                .child(
                    v_flex()
                        .items_center()
                        .gap_2()
                        .child(Spinner::new().small())
                        .child(div().text_xs().text_color(Hsla::white()).child("正在打开…")),
                ),
        )
        .into_any_element()
}
