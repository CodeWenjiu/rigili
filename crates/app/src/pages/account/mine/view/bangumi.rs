use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::BangumiFollow;

use super::{CardRow, MineView};
use crate::shared::browse::filter;
use crate::shared::browse::grid::{
    BangumiCards, CARD_GAP, CardFrame, CardGrid, CardMetrics, CardOverlay, CardStyle, CoverChrome,
    MediaCard, card_grid_loading, card_text_height, media_card, with_opening_overlay,
};
use crate::shared::browse::list::{LIST_FOOTER_HEIGHT, Rows, paged_footer, virtual_rows_tracked};
use crate::shared::browse::season::SeasonOpener;
use crate::shared::ui::state_view::{Layout, empty_state, error_state};

impl MineView {
    pub(super) fn render_bangumi(
        &mut self,
        cinema: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.bangumi_state(cinema);
        let loading = state.loading && state.items.is_empty();
        let error = state.error.clone();
        let empty = state.items.is_empty() && !state.loading;
        let needle = self.search.needle();
        let matched = filter::filter_indices(&state.items, needle.as_deref(), |item, needle| {
            filter::text_matches(&item.title, needle)
        })
        .map_or(state.items.len(), |indices| indices.len());
        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius;

        if loading {
            return card_grid_loading::<BangumiCards>(
                "mine-bangumi-loading",
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
                "mine-bangumi-retry",
                danger,
                cx.listener(move |this, _, _, cx| this.load_bangumi(cinema, cx)),
            );
        }

        if empty {
            return empty_state(Layout::Top, muted, "暂无内容");
        }

        if matched == 0 && !state.loading {
            return empty_state(Layout::Top, muted, "未找到匹配内容");
        }

        self.render_bangumi_list(cinema, window, cx)
    }

    fn render_bangumi_row(
        &self,
        cinema: bool,
        line: CardRow<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let CardRow {
            filtered,
            row,
            columns,
            frame,
        } = line;
        let CardFrame { style, metrics } = frame;
        let state = self.bangumi_state(cinema);
        let count = filtered.map_or(state.items.len(), |indices| indices.len());
        let start = row * columns;
        let end = (start + columns).min(count);
        let opening = self.opening;

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| {
                let item = match filtered {
                    Some(indices) => &state.items[indices[index]],
                    None => &state.items[index],
                };
                let season_id = item.season_id;
                let is_opening = opening == Some(season_id);
                let on_open = cx.listener(move |this, _, _, cx| {
                    this.open_season(season_id, cx);
                });
                bangumi_card(item, is_opening, style, metrics, window, cx, on_open)
            }))
            .into_any_element()
    }

    fn render_bangumi_footer(&self, cinema: bool, cx: &mut Context<Self>) -> AnyElement {
        let retry_id = if cinema {
            ("mine-cinema-retry", 0usize)
        } else {
            ("mine-bangumi-retry", 0usize)
        };

        paged_footer(self.bangumi_state(cinema), retry_id, cx, move |this, cx| {
            this.retry_bangumi_more(cinema, cx)
        })
    }

    fn render_bangumi_list(
        &mut self,
        cinema: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = CardStyle::new(cx);
        let needle = self.search.needle();
        let state = self.bangumi_state(cinema);
        let filtered = filter::filter_indices(&state.items, needle.as_deref(), |item, needle| {
            filter::text_matches(&item.title, needle)
        });
        let count = filtered.as_ref().map_or(state.items.len(), Vec::len);
        let columns = BangumiCards::columns(self.list_width);
        let metrics = BangumiCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        );
        let frame = CardFrame { style, metrics };
        let rows = count.div_ceil(columns);
        let footer = self.bangumi_state(cinema).footer_visible();
        let list_id = if cinema {
            "mine-cinema-list"
        } else {
            "mine-bangumi-list"
        };
        let sizes = Rows::new(rows, metrics.height()).footer_if(footer, LIST_FOOTER_HEIGHT);

        virtual_rows_tracked(
            cx,
            window,
            list_id,
            sizes,
            self.list_width,
            &self.bangumi_state(cinema).scroll,
            move |this, range, window, cx| {
                this.maybe_load_more_bangumi(cinema, range.end.saturating_sub(1), rows, cx);
                range
                    .map(|row| {
                        if footer && row == rows {
                            this.render_bangumi_footer(cinema, cx)
                        } else {
                            this.render_bangumi_row(
                                cinema,
                                CardRow {
                                    filtered: filtered.as_deref(),
                                    row,
                                    columns,
                                    frame,
                                },
                                window,
                                cx,
                            )
                        }
                    })
                    .collect()
            },
        )
    }
}

fn bangumi_card(
    item: &BangumiFollow,
    opening: bool,
    style: CardStyle,
    metrics: CardMetrics,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let latest = item.latest_ep();
    let score = item.score().map(|score| format!("{score:.1}"));
    let footer = if item.progress.is_empty() {
        item.season_type_name.as_str()
    } else {
        item.progress.as_str()
    };
    let card = media_card(
        MediaCard {
            id: SharedString::from(format!("mine-bangumi-{}", item.season_id)),
            cover: &item.cover,
            title: &item.title,
            footer: Some(footer),
            footer_icon: None,
            title_lines: 2,
            thumb_width: metrics.thumb_width,
            author: None,
            video: None,
            chrome: CoverChrome::OnHover,
            overlay: CardOverlay {
                leading: score.as_deref().map(|score| (IconName::Star, score)),
                secondary: None,
                trailing: (!latest.is_empty()).then_some(latest),
                badge: (!item.badge.is_empty()).then_some(item.badge.as_str()),
            },
        },
        style,
        metrics.width,
        metrics.text_height,
        window,
        cx,
        on_open,
    );

    with_opening_overlay(
        card.into_any_element(),
        metrics.width,
        metrics.height(),
        opening,
    )
}
