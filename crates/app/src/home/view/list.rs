use gpui_kit::component::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{HomeItem, HomeTab, HomeView};
use crate::grid::{
    CARD_GAP, CardFrame, CardGrid, CardStyle, VideoCardKind, VideoCards, card_grid_loading,
    card_text_height, live_card, pgc_card, video_card,
};
use crate::list::{LIST_FOOTER_HEIGHT, Rows, list_footer, virtual_rows_tracked};
use crate::state_view::{Layout, empty_state, error_state};

impl HomeView {
    fn render_row(
        &self,
        tab: HomeTab,
        row: usize,
        columns: usize,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(tab);
        let start = row * columns;
        let end = (start + columns).min(state.items.len());
        let opening = self.opening;

        h_flex()
            .h(frame.metrics.height())
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
                if let HomeItem::Video(video) = item {
                    video_card(
                        video,
                        VideoCardKind::Default,
                        frame.style,
                        frame.metrics,
                        window,
                        cx,
                        on_open,
                    )
                } else {
                    home_card(item, is_opening, frame, window, cx, on_open)
                }
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
        let frame = CardFrame { style, metrics };
        let rows = state.items.len().div_ceil(columns);
        let footer = state.loading_more || state.more_error.is_some();
        let sizes = Rows::new(rows, metrics.height()).footer_if(footer, LIST_FOOTER_HEIGHT);

        virtual_rows_tracked(
            cx,
            window,
            tab.list_id(),
            sizes,
            self.list_width,
            &self.state(tab).scroll,
            move |this, range, window, cx| {
                this.maybe_load_more(tab, range.end.saturating_sub(1), rows, cx);
                range
                    .map(|row| {
                        if footer && row == rows {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(tab, row, columns, frame, window, cx)
                        }
                    })
                    .collect()
            },
        )
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
            return card_grid_loading::<VideoCards>(
                "loading",
                self.list_width,
                card_text_height(window),
                window.scale_factor(),
                radius,
            );
        }

        if let Some(error) = error {
            return error_state(
                Layout::Top,
                error,
                "retry",
                danger,
                cx.listener(|this, _, _, cx| this.reload_current(cx)),
            );
        }

        if empty {
            return empty_state(Layout::Top, muted, "暂无内容");
        }

        self.render_list(window, cx)
    }
}

fn home_card(
    item: &HomeItem,
    opening: bool,
    frame: CardFrame,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let CardFrame { style, metrics } = frame;
    let card = match item {
        HomeItem::Live(card) => live_card(card, style, metrics, None, window, cx, on_open),
        HomeItem::Pgc(card) => pgc_card(card, style, metrics, None, window, cx, on_open),
        HomeItem::Video(_) => return div().into_any_element(),
    };

    if !opening {
        return card;
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
