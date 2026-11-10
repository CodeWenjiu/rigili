use gpui_kit::*;
use gpuili_bilibili::SearchResult;

use super::SearchView;
use crate::backend;
use crate::grid::{VideoCard, append_unique};
use crate::platform::runtime::{ApiTaskError, spawn_api};

impl SearchView {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        if self.keyword.is_empty() {
            return;
        }
        if self.backend.is_none() {
            match backend::get(cx) {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    self.loading = false;
                    self.error = Some(error);
                    cx.notify();
                    return;
                }
            }
        }

        let Some(backend) = self.backend.clone() else {
            return;
        };
        let keyword = self.keyword.clone();
        let order = self.order;
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.loading_more = false;
        self.error = None;
        self.more_error = None;
        self.videos.clear();
        self.page = 0;
        self.num_pages = 0;
        self.num_results = 0;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.search(&keyword, 1, order).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(result) => this.apply(result, 1),
                    Err(error) => this.fail(error, cx),
                }
                cx.notify();
            },
        );
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.loading
            || self.loading_more
            || !has_more(self.page, self.num_pages)
            || self.more_error.is_some()
        {
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };

        let keyword = self.keyword.clone();
        let order = self.order;
        let page = self.page + 1;
        let generation = self.generation;
        self.loading_more = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.search(&keyword, page, order).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_more = false;
                match result {
                    Ok(result) => {
                        this.page = page;
                        this.num_pages = result.num_pages;
                        this.num_results = result.num_results;
                        append_unique(
                            &mut this.videos,
                            result.videos.into_iter().map(VideoCard::from).collect(),
                        );
                    }
                    Err(error) => this.more_error = Some(api_error(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn apply(&mut self, result: SearchResult, page: u32) {
        self.page = page;
        self.num_pages = result.num_pages;
        self.num_results = result.num_results;
        self.videos = result.videos.into_iter().map(VideoCard::from).collect();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    pub(super) fn retry_more(&mut self, cx: &mut Context<Self>) {
        self.more_error = None;
        self.load_more(cx);
    }

    pub(super) fn maybe_load_more(
        &mut self,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows > 0 && last_visible + 1 >= rows {
            self.load_more(cx);
        }
    }

    fn fail(&mut self, error: ApiTaskError, cx: &mut Context<Self>) {
        self.error = Some(api_error(error, cx));
    }
}

fn api_error(error: ApiTaskError, cx: &mut App) -> String {
    match error {
        ApiTaskError::Api(error) => {
            if error.is_login_required() {
                crate::login::expire(cx);
            }
            crate::format::api_error_message(&error)
        }
        ApiTaskError::Join(error) => format!("加载任务失败：{error}"),
    }
}

fn has_more(page: u32, num_pages: u32) -> bool {
    page > 0 && page < num_pages
}

#[cfg(test)]
mod tests {
    use super::has_more;

    #[test]
    fn stops_after_the_last_page() {
        assert!(!has_more(0, 10));
        assert!(has_more(1, 10));
        assert!(has_more(9, 10));
        assert!(!has_more(10, 10));
        assert!(!has_more(11, 10));
        assert!(!has_more(1, 0));
    }
}
