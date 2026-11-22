use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(crate) struct Search<V: 'static> {
    input: Entity<InputState>,
    keyword: String,
    on_submit: Submit<V>,
    _subscription: Subscription,
}

type Submit<V> = Rc<dyn Fn(&mut V, &mut Context<V>)>;

impl<V: 'static> Search<V> {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<V>,
        placeholder: impl Into<SharedString>,
        on_submit: impl Fn(&mut V, &mut Context<V>) + 'static,
    ) -> Self {
        let placeholder = placeholder.into();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let on_submit = Rc::new(on_submit);
        let callback = on_submit.clone();
        let subscription = cx.subscribe_in(&input, window, move |this, input, event, _, cx| {
            let submit = match event {
                InputEvent::PressEnter { .. } => true,
                InputEvent::Change => input.read(cx).value().is_empty(),
                _ => false,
            };
            if submit {
                callback(this, cx);
            }
        });

        Self {
            input,
            keyword: String::new(),
            on_submit,
            _subscription: subscription,
        }
    }

    pub(crate) fn keyword(&self) -> &str {
        &self.keyword
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.keyword.is_empty()
    }

    pub(crate) fn needle(&self) -> Option<String> {
        (!self.keyword.is_empty()).then(|| self.keyword.to_lowercase())
    }

    pub(crate) fn submit(&mut self, cx: &App) -> bool {
        let keyword = self.input.read(cx).value().trim().to_owned();
        if keyword == self.keyword {
            return false;
        }
        self.keyword = keyword;
        true
    }

    pub(crate) fn clear(&mut self, window: &mut Window, cx: &mut App) {
        self.keyword.clear();
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(crate) fn field(&self, cx: &mut Context<V>) -> AnyElement {
        let input = self.input.clone();
        let on_submit = self.on_submit.clone();
        let submit = Button::new("filter-submit")
            .ghost()
            .xsmall()
            .icon(IconName::Search)
            .accessibility_label("搜索")
            .tooltip("搜索")
            .on_click(cx.listener(move |this, _, window, cx| {
                on_submit(this, cx);
                input.update(cx, |input, cx| input.focus(window, cx));
            }));

        div()
            .w_full()
            .key_context(rigili_core::TEXT_INPUT_CONTEXT)
            .child(
                Input::new(&self.input)
                    .w_full()
                    .large()
                    .h(px(40.))
                    .cleanable(true)
                    .suffix(submit),
            )
            .into_any_element()
    }
}

pub(crate) fn text_matches(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(needle)
}

pub(crate) fn matching<T>(
    items: impl IntoIterator<Item = T>,
    needle: Option<&str>,
    matches: impl Fn(&T, &str) -> bool,
) -> Vec<T> {
    match needle {
        Some(needle) => items
            .into_iter()
            .filter(|item| matches(item, needle))
            .collect(),
        None => items.into_iter().collect(),
    }
}

pub(crate) fn filter_indices<T>(
    items: &[T],
    needle: Option<&str>,
    matches: impl Fn(&T, &str) -> bool,
) -> Option<Vec<usize>> {
    needle.map(|needle| {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches(item, needle))
            .map(|(index, _)| index)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{matching, text_matches};

    #[test]
    fn matches_filters_case_insensitively_and_keeps_everything_without_needle() {
        assert!(text_matches("Rust 入门", "rust"));
        assert!(text_matches("RUST 入门", "rust"));
        assert!(!text_matches("Go 入门", "rust"));

        let items = ["rust", "go"];
        let kept = matching(items, None, |item, needle| text_matches(item, needle));
        assert_eq!(kept, items);

        let kept = matching(items, Some("rust"), |item, needle| {
            text_matches(item, needle)
        });
        assert_eq!(kept, ["rust"]);
    }
}
