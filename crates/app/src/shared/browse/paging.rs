use std::collections::HashSet;
use std::future::Future;

use gpui_kit::*;
use rigili_bilibili::ApiError;
use rigili_core::api_task_error_message;
use rigili_core::runtime::spawn_api;

use crate::shared::browse::list::ListScroll;

pub(crate) struct Page<T, M = ()> {
    pub(crate) items: Vec<T>,
    pub(crate) has_more: bool,
    pub(crate) meta: M,
}

impl<T, M> Page<T, M> {
    pub(crate) fn new(items: impl IntoIterator<Item = T>, has_more: bool, meta: M) -> Self {
        Self {
            items: items.into_iter().collect(),
            has_more,
            meta,
        }
    }
}

pub(crate) struct Paged<T, M = ()> {
    pub(crate) items: Vec<T>,
    seen: HashSet<String>,
    pub(crate) meta: M,
    pub(crate) has_more: bool,
    pub(crate) loading: bool,
    pub(crate) loading_more: bool,
    pub(crate) loaded: bool,
    pub(crate) error: Option<String>,
    pub(crate) more_error: Option<String>,
    pub(crate) generation: u64,
    pub(crate) scroll: ListScroll,
}

impl<T, M: Default> Paged<T, M> {
    pub(crate) fn new() -> Self {
        Self {
            items: Vec::new(),
            seen: HashSet::new(),
            meta: M::default(),
            has_more: false,
            loading: false,
            loading_more: false,
            loaded: false,
            error: None,
            more_error: None,
            generation: 0,
            scroll: ListScroll::new(),
        }
    }
}

impl<T, M: Clone> Paged<T, M> {
    pub(crate) fn reset(&mut self, meta: M) -> u64 {
        self.generation += 1;
        self.items.clear();
        self.seen.clear();
        self.meta = meta;
        self.has_more = false;
        self.loading = true;
        self.loading_more = false;
        self.loaded = false;
        self.error = None;
        self.more_error = None;
        self.generation
    }

    pub(crate) fn start_more(&mut self) -> Option<(M, u64)> {
        if self.loading || self.loading_more || !self.has_more || self.more_error.is_some() {
            return None;
        }
        self.loading_more = true;
        Some((self.meta.clone(), self.generation))
    }

    pub(crate) fn apply_first(&mut self, page: Page<T, M>) {
        self.loading = false;
        self.loaded = true;
        self.error = None;
        self.more_error = None;
        self.has_more = page.has_more;
        self.meta = page.meta;
        self.items = page.items;
        self.seen.clear();
    }

    pub(crate) fn apply_first_unique(&mut self, page: Page<T, M>, key: impl Fn(&T) -> String) {
        self.apply_first(page);
        self.seen = index_by(&self.items, key);
    }

    pub(crate) fn apply_more_unique(&mut self, page: Page<T, M>, key: impl Fn(&T) -> String) {
        self.loading_more = false;
        self.more_error = None;
        self.has_more = page.has_more;
        self.meta = page.meta;
        append_unique_by(&mut self.items, &mut self.seen, page.items, key);
    }

    pub(crate) fn fail_first(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub(crate) fn fail_first_reset(&mut self, message: String)
    where
        M: Default,
    {
        self.reset(M::default());
        self.fail_first(message);
    }

    pub(crate) fn fail_more(&mut self, message: String) {
        self.loading_more = false;
        self.more_error = Some(message);
    }

    pub(crate) fn retry_more(&mut self) {
        self.more_error = None;
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

pub(crate) fn spawn_first<V, T, M, F, S, A>(
    this: &mut V,
    cx: &mut Context<V>,
    slot: S,
    key: impl Fn(&T) -> String + 'static,
    future: F,
    on_applied: A,
) -> u64
where
    V: 'static,
    T: Send + 'static,
    M: Clone + Default + Send + 'static,
    S: for<'a> Fn(&'a mut V) -> &'a mut Paged<T, M> + Copy + 'static,
    F: Future<Output = Result<Page<T, M>, ApiError>> + Send + 'static,
    A: FnOnce(&mut V) + 'static,
{
    let generation = slot(this).reset(M::default());
    cx.notify();

    spawn_api(cx, future, move |this, result, cx| {
        let applied = {
            let state = slot(this);
            if !state.is_current(generation) {
                return;
            }
            match result {
                Ok(page) => {
                    state.apply_first_unique(page, key);
                    true
                }
                Err(error) => {
                    state.fail_first(api_task_error_message(error, cx));
                    false
                }
            }
        };
        if applied {
            on_applied(this);
        }
        cx.notify();
    });

    generation
}

pub(crate) fn spawn_more<V, T, M, F, S, A>(
    this: &mut V,
    cx: &mut Context<V>,
    slot: S,
    key: impl Fn(&T) -> String + 'static,
    fetch: impl FnOnce(M) -> F,
    on_applied: A,
) where
    V: 'static,
    T: Send + 'static,
    M: Clone + Send + 'static,
    S: for<'a> Fn(&'a mut V) -> &'a mut Paged<T, M> + Copy + 'static,
    F: Future<Output = Result<Page<T, M>, ApiError>> + Send + 'static,
    A: FnOnce(&mut V) + 'static,
{
    let Some((meta, generation)) = slot(this).start_more() else {
        return;
    };
    let future = fetch(meta);
    cx.notify();

    spawn_api(cx, future, move |this, result, cx| {
        let applied = {
            let state = slot(this);
            if !state.is_current(generation) {
                return;
            }
            match result {
                Ok(page) => {
                    state.apply_more_unique(page, key);
                    true
                }
                Err(error) => {
                    state.fail_more(api_task_error_message(error, cx));
                    false
                }
            }
        };
        if applied {
            on_applied(this);
        }
        cx.notify();
    });
}

pub(crate) fn append_unique_by<T>(
    items: &mut Vec<T>,
    seen: &mut HashSet<String>,
    incoming: Vec<T>,
    key: impl Fn(&T) -> String,
) {
    items.extend(incoming.into_iter().filter(|item| seen.insert(key(item))));
}

pub(crate) fn index_by<T>(items: &[T], key: impl Fn(&T) -> String) -> HashSet<String> {
    items.iter().map(key).collect()
}

pub(crate) fn dedupe_by<T>(
    items: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> String,
) -> Vec<T> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(key(item)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Page, Paged, append_unique_by, index_by};

    #[test]
    fn replaces_the_first_page_and_dedupes_appends() {
        let mut state = Paged::<String, u32>::new();
        let generation = state.reset(0);
        assert!(state.loading);
        state.apply_first_unique(
            Page::new(["a".to_owned(), "b".to_owned()], true, 1),
            |item| item.clone(),
        );
        assert_eq!(state.items, ["a", "b"]);
        assert!(state.loaded);
        assert!(!state.loading);
        assert_eq!(state.meta, 1);

        let (meta, more_generation) = state.start_more().expect("more");
        assert_eq!(meta, 1);
        assert_eq!(more_generation, generation);
        assert!(state.loading_more);
        state.apply_more_unique(
            Page::new(["b".to_owned(), "c".to_owned()], false, 2),
            |item| item.clone(),
        );
        assert_eq!(state.items, ["a", "b", "c"]);
        assert_eq!(state.meta, 2);
        assert!(!state.loading_more);
        assert!(!state.has_more);
    }

    #[test]
    fn blocks_more_while_loading_or_after_failure() {
        let mut state = Paged::<String, u32>::new();
        state.reset(0);
        state.apply_first(Page::new(["a".to_owned()], true, 1));
        state.fail_more("错误".into());
        assert!(state.start_more().is_none());

        state.retry_more();
        assert!(state.more_error.is_none());
        assert!(state.start_more().is_some());

        state.reset(0);
        assert!(state.start_more().is_none());
    }

    #[test]
    fn appends_only_new_items_by_key() {
        let mut items = vec!["BV1".to_owned()];
        let mut seen = index_by(&items, |item| item.clone());
        append_unique_by(
            &mut items,
            &mut seen,
            vec!["BV1".to_owned(), "BV2".to_owned()],
            |item| item.clone(),
        );
        assert_eq!(items, ["BV1", "BV2"]);
    }
}
