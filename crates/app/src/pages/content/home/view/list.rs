use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{HomeItem, HomeTab, HomeView};
use crate::shared::browse::grid::{
    CARD_GAP, CardFrame, CardGrid, CardStyle, CoverChrome, VideoCardKind, VideoCards,
    card_grid_loading, card_text_height, live_card, pgc_card, thumb_bytes, video_card,
    with_opening_overlay,
};
use crate::shared::browse::list::{LIST_FOOTER_HEIGHT, Rows, paged_footer, virtual_rows_tracked};
use crate::shared::ui::state_view::{Layout, empty_state, error_state};
use rigili_bilibili::thumbnail;
use rigili_core::image_cache;

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
                    let id = video.id().clone();
                    let chrome = if self.hovered.as_ref() == Some(&id) {
                        CoverChrome::Shown
                    } else {
                        CoverChrome::Hidden
                    };
                    let card = video_card(
                        video,
                        VideoCardKind::Default,
                        frame,
                        chrome,
                        window,
                        cx,
                        on_open,
                    );
                    div()
                        .id(gpui_kit::ElementId::NamedInteger(id.clone(), 9))
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            this.set_hovered(hovered.then(|| id.clone()), cx);
                        }))
                        .child(card)
                        .into_any_element()
                } else {
                    home_card(item, is_opening, frame, window, cx, on_open)
                }
            }))
            .into_any_element()
    }

    fn render_footer(&self, tab: HomeTab, cx: &mut Context<Self>) -> AnyElement {
        paged_footer(
            self.state(tab),
            ("home-retry-more", tab.index()),
            cx,
            move |this, cx| this.retry_more(tab, cx),
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
        let rows = state.page_rows(columns);
        let footer = state.footer_visible();
        let sizes = Rows::new(rows, metrics.height()).footer_if(footer, LIST_FOOTER_HEIGHT);

        let row_height = f32::from(metrics.height()).max(1.);
        let viewport_rows =
            (f32::from(window.viewport_size().height) / row_height).ceil() as usize + 1;
        image_cache::tune(
            cx,
            window,
            columns * viewport_rows,
            thumb_bytes(metrics.thumb_width),
        );

        virtual_rows_tracked(
            cx,
            window,
            tab.list_id(),
            sizes,
            self.list_width,
            &self.state(tab).scroll,
            move |this, range, window, cx| {
                let first = range.start;
                let last = range.end;
                let count = range.len();
                this.maybe_load_more(tab, range.end.saturating_sub(1), rows, cx);
                let elements = range
                    .map(|row| {
                        if footer && row == rows {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(tab, row, columns, frame, window, cx)
                        }
                    })
                    .collect::<Vec<_>>();
                if this.prefetched != Some((tab, first, last)) {
                    let forward = match this.prefetched {
                        Some((previous, previous_first, _)) => {
                            previous != tab || first >= previous_first
                        }
                        None => true,
                    };
                    this.prefetched = Some((tab, first, last));
                    let ahead = if forward {
                        last..(last + count).min(rows)
                    } else {
                        first.saturating_sub(count)..first
                    };
                    Self::prefetch_covers(
                        &this.state(tab).items,
                        columns,
                        frame.metrics.thumb_width,
                        ahead,
                        image_cache::room(cx),
                        window,
                        cx,
                    );
                }
                elements
            },
        )
    }

    fn prefetch_covers(
        items: &[HomeItem],
        columns: usize,
        thumb_width: u32,
        rows: std::ops::Range<usize>,
        room: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut room = room;
        for row in rows {
            let start = row * columns;
            if start >= items.len() || room == 0 {
                return;
            }
            let stop = (start + columns).min(items.len());
            for item in &items[start..stop] {
                if room == 0 {
                    return;
                }
                if let Some(cover) = item.cover() {
                    image_cache::prefetch(cx, window, &thumbnail(cover, thumb_width));
                    room -= 1;
                }
            }
        }
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

    with_opening_overlay(card, metrics.width, metrics.height(), opening)
}
