use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{SeasonMeta, SeriesMeta, thumbnail};
use rigili_core::api_task_error_message;
use rigili_core::format_count;
use rigili_core::runtime::spawn_api;

use super::SpaceView;
use crate::shared::browse::filter;
use crate::shared::browse::grid::{CARD_GAP, CardGrid, FolderCards};
use crate::shared::browse::list::track_element_width;
use crate::shared::ui::state_view::{Layout, centered_spinner, error_state};

impl SpaceView {
    pub(super) fn load_collections(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if !reset && (self.collections.loading || self.collections.loaded) {
            return;
        }
        let client = match self.cached_client(cx) {
            Ok(client) => client,
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
            async move { client.collections(mid, 1).await },
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

        crate::shared::ui::scroll::attach(
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
}

struct CollectionCard {
    name: String,
    cover: String,
    total: u64,
    kind: &'static str,
}

impl CollectionCard {
    fn matches(&self, needle: &str) -> bool {
        crate::shared::browse::filter::text_matches(&self.name, needle)
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

#[cfg(test)]
mod tests {
    use rigili_bilibili::SeasonMeta;

    use super::season_cards;
    use crate::shared::browse::filter;

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
