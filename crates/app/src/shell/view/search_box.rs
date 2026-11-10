use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Align, Placement, Positioner};
use gpui_kit::component::button::*;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{HotBadge, HotWord, SearchSuggestion};

use super::MainView;
use super::search_history::SearchHistory;
use crate::backend;
use crate::platform::runtime::spawn_api;

const HISTORY_COLLAPSED: usize = 10;
const SUGGESTION_LIMIT: usize = 10;
const SUGGESTION_DEBOUNCE: Duration = Duration::from_millis(160);

pub(super) struct SearchBox {
    open: bool,
    suppress_focus_open: bool,
    suppress_change: bool,
    history: SearchHistory,
    history_expanded: bool,
    hot: Vec<HotWord>,
    hot_loading: bool,
    hot_loaded: bool,
    suggestions: Vec<SearchSuggestion>,
    suggestions_loading: bool,
    suggestion_generation: u64,
}

impl SearchBox {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            suppress_focus_open: false,
            suppress_change: false,
            history: SearchHistory::load(),
            history_expanded: false,
            hot: Vec::new(),
            hot_loading: false,
            hot_loaded: false,
            suggestions: Vec::new(),
            suggestions_loading: false,
            suggestion_generation: 0,
        }
    }

    pub(super) fn close(&mut self) {
        self.open = false;
    }

    pub(super) fn record(&mut self, keyword: &str) {
        self.history.record(keyword);
    }

    pub(super) fn suppress_next_focus_open(&mut self) {
        self.suppress_focus_open = true;
    }
}

impl MainView {
    pub(super) fn handle_search_input(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::PressEnter { .. } => self.submit_search(cx),
            InputEvent::Focus => self.search_focused(cx),
            InputEvent::Blur => {}
            InputEvent::Change => self.search_changed(cx),
        }
    }

    fn search_focused(&mut self, cx: &mut Context<Self>) {
        if self.search_box.suppress_focus_open {
            self.search_box.suppress_focus_open = false;
            return;
        }
        self.open_search_box(cx);
    }

    pub(super) fn open_search_box(&mut self, cx: &mut Context<Self>) {
        if self.search_box.open {
            return;
        }
        self.search_box.open = true;
        self.load_hot_words(cx);
        self.refresh_suggestions(cx);
        cx.notify();
    }

    pub(super) fn close_search_box(&mut self, cx: &mut Context<Self>) {
        self.search_box.open = false;
        cx.notify();
    }

    pub(super) fn search_box_is_open(&self) -> bool {
        self.search_box.open
    }

    fn search_changed(&mut self, cx: &mut Context<Self>) {
        if self.search_box.suppress_change {
            return;
        }
        if !self.search_box.open {
            self.search_box.open = true;
            self.load_hot_words(cx);
        }
        self.refresh_suggestions(cx);
        cx.notify();
    }

    fn refresh_suggestions(&mut self, cx: &mut Context<Self>) {
        let term = self.search_input.read(cx).value().trim().to_owned();
        if term.is_empty() {
            self.search_box.suggestion_generation += 1;
            self.search_box.suggestions.clear();
            self.search_box.suggestions_loading = false;
            return;
        }
        self.request_suggestions(term, cx);
    }

    fn request_suggestions(&mut self, term: String, cx: &mut Context<Self>) {
        let Ok(backend) = backend::get(cx) else {
            return;
        };
        self.search_box.suggestion_generation += 1;
        let generation = self.search_box.suggestion_generation;
        self.search_box.suggestions_loading = true;
        cx.notify();

        spawn_api(
            cx,
            async move {
                tokio::time::sleep(SUGGESTION_DEBOUNCE).await;
                backend.search_suggestions(&term).await
            },
            move |this, result, cx| {
                if this.search_box.suggestion_generation != generation {
                    return;
                }
                this.search_box.suggestions_loading = false;
                match result {
                    Ok(mut suggestions) => {
                        suggestions.items.truncate(SUGGESTION_LIMIT);
                        this.search_box.suggestions = suggestions.items;
                    }
                    Err(_) => this.search_box.suggestions.clear(),
                }
                cx.notify();
            },
        );
    }

    fn load_hot_words(&mut self, cx: &mut Context<Self>) {
        if self.search_box.hot_loading || self.search_box.hot_loaded {
            return;
        }
        let Ok(backend) = backend::get(cx) else {
            return;
        };
        self.search_box.hot_loading = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.hot_words().await },
            move |this, result, cx| {
                this.search_box.hot_loading = false;
                if let Ok(hot) = result {
                    this.search_box.hot = hot.items;
                    this.search_box.hot_loaded = true;
                }
                cx.notify();
            },
        );
    }

    pub(super) fn pick_search(
        &mut self,
        keyword: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_box.suppress_change = true;
        self.search_input
            .update(cx, |input, cx| input.set_value(keyword, window, cx));
        self.search_box.suppress_change = false;
        self.search_box.open = false;
        self.submit_search(cx);
        self.focus_search(window, cx);
        cx.notify();
    }

    pub(super) fn render_search_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_box.open {
            return None;
        }
        let bounds = self.search_bounds.get()?;
        let query = self.search_input.read(cx).value();
        let query_empty = query.trim().is_empty();

        if query_empty {
            let has_idle = !self.search_box.history.is_empty()
                || !self.search_box.hot.is_empty()
                || self.search_box.hot_loading;
            if !has_idle {
                return None;
            }
        } else if self.search_box.suggestions.is_empty() && !self.search_box.suggestions_loading {
            return None;
        }

        let content = if query_empty {
            self.render_search_idle(cx)
        } else {
            self.render_suggestions(cx)
        };

        let panel = v_flex()
            .id("search-panel")
            .occlude()
            .w(bounds.size.width)
            .max_h(px(560.))
            .overflow_y_scroll()
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius_lg)
            .p_4()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                let inside_input = this
                    .search_bounds
                    .get()
                    .is_some_and(|bounds| bounds.contains(&event.position));
                if !inside_input {
                    this.close_search_box(cx);
                }
            }))
            .child(content);

        Some(
            deferred(
                Positioner::side(bounds)
                    .placement(Placement::Bottom)
                    .align(Align::Start)
                    .offset(px(6.))
                    .margin(px(8.))
                    .occlude()
                    .child(panel),
            )
            .into_any_element(),
        )
    }

    fn render_search_idle(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_4()
            .when(!self.search_box.history.is_empty(), |this| {
                this.child(self.render_history(cx))
            })
            .when(
                !self.search_box.hot.is_empty() || self.search_box.hot_loading,
                |this| this.child(self.render_hot(cx)),
            )
            .into_any_element()
    }

    fn render_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let total = self.search_box.history.keywords().len();
        let visible = if self.search_box.history_expanded {
            total
        } else {
            total.min(HISTORY_COLLAPSED)
        };
        let keywords = self.search_box.history.keywords()[..visible].to_vec();
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.search_box.history.clear();
                                this.search_box.history_expanded = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .children(keywords.into_iter().enumerate().map(|(index, keyword)| {
                        let term = keyword.clone();
                        Button::new(("history-chip", index))
                            .custom(history_chip(cx))
                            .small()
                            .label(keyword)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.pick_search(term.clone(), window, cx);
                            }))
                    })),
            )
            .when(total > HISTORY_COLLAPSED, |this| {
                this.child(
                    h_flex().justify_center().child(
                        Button::new("toggle-search-history")
                            .text()
                            .small()
                            .text_color(muted)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.search_box.history_expanded =
                                    !this.search_box.history_expanded;
                                cx.notify();
                            }))
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

    fn render_hot(&self, cx: &mut Context<Self>) -> AnyElement {
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
                .child(self.render_hot_column(&self.search_box.hot[..split], 0, cx))
                .child(self.render_hot_column(&self.search_box.hot[split..], split, cx))
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .children(
                words
                    .iter()
                    .enumerate()
                    .map(|(index, word)| self.render_hot_row(offset + index, word, cx)),
            )
            .into_any_element()
    }

    fn render_hot_row(&self, index: usize, word: &HotWord, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let hover = cx.theme().accent;
        let keyword = word.keyword.clone();
        let name = word.name.clone();
        let badge = word.badge;

        h_flex()
            .id(("hot-word", index))
            .h(px(36.))
            .items_center()
            .gap_2()
            .px_2()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .hover(move |this| this.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_search(keyword.clone(), window, cx);
            }))
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

    fn render_suggestions(&self, cx: &mut Context<Self>) -> AnyElement {
        let foreground = cx.theme().foreground;
        let primary = cx.theme().primary;
        let hover = cx.theme().accent;
        let muted = cx.theme().muted_foreground;

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

                        h_flex()
                            .id(("search-suggestion", index))
                            .h(px(40.))
                            .items_center()
                            .px_3()
                            .rounded(cx.theme().radius)
                            .cursor_pointer()
                            .hover(move |this| this.bg(hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.pick_search(value.clone(), window, cx);
                            }))
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
            .when(
                self.search_box.suggestions.is_empty() && self.search_box.suggestions_loading,
                |this| {
                    this.child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .p_2()
                            .child(Spinner::new().small())
                            .child(div().text_xs().text_color(muted).child("正在联想…")),
                    )
                },
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
