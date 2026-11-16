rigili_macro::mod_prv!(panel);

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{HotWord, SearchSuggestion};

use super::MainView;
use super::search_history::SearchHistory;
use rigili_core::client;
use rigili_core::runtime::spawn_api;

const HISTORY_COLLAPSED: usize = 10;
const SUGGESTION_LIMIT: usize = 10;
const SUGGESTION_DEBOUNCE: Duration = Duration::from_millis(160);

pub(super) struct SearchBox {
    input: Entity<InputState>,
    _input_sub: Subscription,
    open: bool,
    suppress_focus_open: bool,
    suppress_change: bool,
    history: SearchHistory,
    history_expanded: bool,
    hot: Vec<HotWord>,
    hot_loading: bool,
    hot_loaded: bool,
    suggestions: Vec<SearchSuggestion>,
    active_suggestion: Option<usize>,
    suggestion_generation: u64,
}

impl SearchBox {
    pub(super) fn new(window: &mut Window, cx: &mut Context<MainView>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索视频、番剧、用户"));
        let input_sub = cx.subscribe_in(&input, window, |this, _, event, _, cx| {
            this.handle_search_input(event, cx);
        });
        Self {
            input,
            _input_sub: input_sub,
            open: false,
            suppress_focus_open: false,
            suppress_change: false,
            history: SearchHistory::load(),
            history_expanded: false,
            hot: Vec::new(),
            hot_loading: false,
            hot_loaded: false,
            suggestions: Vec::new(),
            active_suggestion: None,
            suggestion_generation: 0,
        }
    }

    pub(super) fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    pub(super) fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub(super) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    pub(super) fn set_value(&self, value: String, window: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_value(value, window, cx));
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.active_suggestion = None;
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

    pub(super) fn dismiss_search_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.search_box.focus_handle(cx).is_focused(window);
        self.search_box.close();
        if focused {
            window.blur(cx);
            let focus = self.focus.clone();
            window.defer(cx, move |window, cx| {
                if window.focused(cx).is_none() {
                    focus.focus(window, cx);
                }
            });
        }
        cx.notify();
    }

    pub(super) fn search_box_is_open(&self) -> bool {
        self.search_box.open
    }

    pub(super) fn move_suggestion_selection(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        if !self.search_box.open || self.search_box.suggestions.is_empty() {
            return false;
        }
        let next = next_selection(
            self.search_box.active_suggestion,
            self.search_box.suggestions.len(),
            down,
        );
        if next != self.search_box.active_suggestion {
            self.search_box.active_suggestion = next;
            cx.notify();
        }
        true
    }

    pub(super) fn pick_active_suggestion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.search_box.active_suggestion else {
            return false;
        };
        let Some(suggestion) = self.search_box.suggestions.get(index).cloned() else {
            return false;
        };
        self.pick_search(suggestion.value, window, cx);
        true
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
        self.search_box.active_suggestion = None;
        let term = self.search_box.value(cx).trim().to_owned();
        if term.is_empty() {
            self.search_box.suggestion_generation += 1;
            self.search_box.suggestions.clear();
            return;
        }
        self.request_suggestions(term, cx);
    }

    fn request_suggestions(&mut self, term: String, cx: &mut Context<Self>) {
        let Ok(client) = client::get(cx) else {
            return;
        };
        self.search_box.suggestion_generation += 1;
        let generation = self.search_box.suggestion_generation;

        spawn_api(
            cx,
            async move {
                tokio::time::sleep(SUGGESTION_DEBOUNCE).await;
                client.search_suggestions(&term).await
            },
            move |this, result, cx| {
                if this.search_box.suggestion_generation != generation {
                    return;
                }
                match result {
                    Ok(mut suggestions) => {
                        suggestions.items.truncate(SUGGESTION_LIMIT);
                        this.search_box.suggestions = suggestions.items;
                    }
                    Err(_) => this.search_box.suggestions.clear(),
                }
                this.search_box.active_suggestion = None;
                cx.notify();
            },
        );
    }

    fn load_hot_words(&mut self, cx: &mut Context<Self>) {
        if self.search_box.hot_loading || self.search_box.hot_loaded {
            return;
        }
        let Ok(client) = client::get(cx) else {
            return;
        };
        self.search_box.hot_loading = true;
        cx.notify();

        spawn_api(
            cx,
            async move { client.hot_words().await },
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
        self.search_box.set_value(keyword, window, cx);
        self.search_box.suppress_change = false;
        self.search_box.open = false;
        self.submit_search(cx);
        self.focus_search(window, cx);
        cx.notify();
    }
}

fn next_selection(current: Option<usize>, count: usize, down: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(if down {
        current.map_or(0, |index| (index + 1).min(count - 1))
    } else {
        current.map_or(count - 1, |index| index.saturating_sub(1))
    })
}

#[cfg(test)]
mod tests {
    use super::next_selection;

    #[test]
    fn selects_and_clamps_suggestions() {
        assert_eq!(next_selection(None, 3, true), Some(0));
        assert_eq!(next_selection(None, 3, false), Some(2));
        assert_eq!(next_selection(None, 0, true), None);
        assert_eq!(next_selection(Some(0), 0, false), None);
        assert_eq!(next_selection(Some(0), 3, true), Some(1));
        assert_eq!(next_selection(Some(2), 3, true), Some(2));
        assert_eq!(next_selection(Some(0), 3, false), Some(0));
        assert_eq!(next_selection(Some(2), 3, false), Some(1));
    }
}
