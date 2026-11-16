use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{HotBadge, HotWord};

use super::{HISTORY_COLLAPSED, MainView};
use crate::app::preferences;

impl MainView {
    pub(crate) fn render_search_panel(
        &self,
        weak: &WeakEntity<Self>,
        width: Pixels,
        cx: &App,
    ) -> AnyElement {
        if !self.search_box.open {
            return div().into_any_element();
        }

        let query = self.search_box.value(cx);
        let query_empty = query.trim().is_empty();
        let has_content = if query_empty {
            !preferences::search_history(cx).is_empty()
                || !self.search_box.hot.is_empty()
                || self.search_box.hot_loading
        } else {
            !self.search_box.suggestions.is_empty()
        };
        let content = if query_empty {
            self.render_search_idle(weak, cx)
        } else {
            self.render_suggestions(weak, cx)
        };

        v_flex()
            .id("search-panel")
            .occlude()
            .w(width)
            .max_h(px(560.))
            .overflow_y_scroll()
            .when(has_content, |this| this.popover_style(cx).p_4())
            .when(!has_content, |this| this.h(px(0.)))
            .child(v_flex().id("search-content").w_full().child(content))
            .into_any_element()
    }

    fn render_search_idle(&self, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        v_flex()
            .gap_4()
            .when(!preferences::search_history(cx).is_empty(), |this| {
                this.child(self.render_history(weak, cx))
            })
            .when(
                !self.search_box.hot.is_empty() || self.search_box.hot_loading,
                |this| this.child(self.render_hot(weak, cx)),
            )
            .into_any_element()
    }

    fn render_history(&self, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let history = preferences::search_history(cx);
        let total = history.len();
        let visible = if self.search_box.history_expanded {
            total
        } else {
            total.min(HISTORY_COLLAPSED)
        };
        let keywords = history[..visible].to_vec();
        let expanded = self.search_box.history_expanded;

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("搜索历史"),
                    )
                    .child(
                        Button::new("clear-search-history")
                            .text()
                            .small()
                            .text_color(muted)
                            .label("清空")
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        preferences::clear_search_history(cx);
                                        this.search_box.history_expanded = false;
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .children(keywords.into_iter().enumerate().map(|(index, keyword)| {
                        let term = keyword.clone();
                        let weak = weak.clone();
                        Button::new(("history-chip", index))
                            .custom(history_chip(cx))
                            .small()
                            .label(keyword)
                            .on_click(move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.pick_search(term.clone(), window, cx);
                                });
                            })
                    })),
            )
            .when(total > HISTORY_COLLAPSED, |this| {
                this.child(
                    h_flex().justify_center().child(
                        Button::new("toggle-search-history")
                            .text()
                            .small()
                            .text_color(muted)
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.search_box.history_expanded =
                                            !this.search_box.history_expanded;
                                        cx.notify();
                                    });
                                }
                            })
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(if expanded { "收起" } else { "展开更多" })
                                    .child(
                                        Icon::new(if expanded {
                                            IconName::ChevronUp
                                        } else {
                                            IconName::ChevronDown
                                        })
                                        .with_size(px(14.)),
                                    ),
                            ),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_hot(&self, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        let body = if self.search_box.hot.is_empty() {
            h_flex()
                .items_center()
                .gap_2()
                .py_2()
                .child(Spinner::new().small())
                .child(div().text_xs().text_color(muted).child("正在加载热搜…"))
                .into_any_element()
        } else {
            let split = self.search_box.hot.len().div_ceil(2);
            h_flex()
                .items_start()
                .gap_4()
                .child(self.render_hot_column(&self.search_box.hot[..split], 0, weak, cx))
                .child(self.render_hot_column(&self.search_box.hot[split..], split, weak, cx))
                .into_any_element()
        };

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("热搜"),
            )
            .child(body)
            .into_any_element()
    }

    fn render_hot_column(
        &self,
        words: &[HotWord],
        offset: usize,
        weak: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .children(
                words
                    .iter()
                    .enumerate()
                    .map(|(index, word)| self.render_hot_row(offset + index, word, weak, cx)),
            )
            .into_any_element()
    }

    fn render_hot_row(
        &self,
        index: usize,
        word: &HotWord,
        weak: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let hover = cx.theme().accent;
        let keyword = word.keyword.clone();
        let name = word.name.clone();
        let badge = word.badge;
        let weak = weak.clone();

        h_flex()
            .id(("hot-word", index))
            .h(px(36.))
            .items_center()
            .gap_2()
            .px_2()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .hover(move |this| this.bg(hover))
            .on_click(move |_, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.pick_search(keyword.clone(), window, cx);
                });
            })
            .child(
                div()
                    .w(px(20.))
                    .text_sm()
                    .text_color(if index < 3 { foreground } else { muted })
                    .child((index + 1).to_string()),
            )
            .child(div().flex_1().min_w_0().truncate().text_sm().child(name))
            .when_some(badge, |this, badge| this.child(hot_badge(badge, cx)))
            .into_any_element()
    }

    fn render_suggestions(&self, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let foreground = cx.theme().foreground;
        let primary = cx.theme().primary;
        let hover = cx.theme().accent;

        v_flex()
            .gap_1()
            .children(
                self.search_box
                    .suggestions
                    .iter()
                    .enumerate()
                    .map(|(index, suggestion)| {
                        let value = suggestion.value.clone();
                        let segments = suggestion.segments.clone();
                        let active = self.search_box.active_suggestion == Some(index);
                        let weak = weak.clone();

                        h_flex()
                            .id(("search-suggestion", index))
                            .h(px(40.))
                            .items_center()
                            .px_3()
                            .rounded(cx.theme().radius)
                            .cursor_pointer()
                            .when(active, |this| this.bg(hover))
                            .hover(move |this| this.bg(hover))
                            .on_click(move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.pick_search(value.clone(), window, cx);
                                });
                            })
                            .children(segments.into_iter().map(|segment| {
                                div()
                                    .text_sm()
                                    .text_color(if segment.highlight {
                                        primary
                                    } else {
                                        foreground
                                    })
                                    .child(segment.text)
                            }))
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }
}
fn history_chip(cx: &App) -> ButtonCustomVariant {
    let theme = cx.theme();
    ButtonCustomVariant::new(cx)
        .color(theme.accent)
        .foreground(theme.accent_foreground)
        .hover(theme.accent.mix_oklab(theme.foreground, 0.08))
        .active(theme.accent.mix_oklab(theme.foreground, 0.14))
}

fn hot_badge(badge: HotBadge, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (background, foreground, label) = match badge {
        HotBadge::New => (theme.warning, theme.warning_foreground, "新"),
        HotBadge::Hot => (theme.primary, theme.primary_foreground, "热"),
    };

    div()
        .px_1()
        .rounded(px(3.))
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .child(label)
        .into_any_element()
}
