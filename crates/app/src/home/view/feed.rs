use gpui_kit::*;
use gpuili_bilibili::Popular;

use super::HomeView;
use crate::backend;
use crate::grid::{VideoCard, append_unique};
use crate::platform::runtime::{ApiTaskError, spawn_api};

const PAGE_SIZE: u32 = 30;

impl HomeView {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
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
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.loading_more = false;
        self.error = None;
        self.more_error = None;
        self.videos.clear();
        self.page = 0;
        self.has_more = false;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.popular(1, PAGE_SIZE).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(popular) => {
                        this.page = 1;
                        this.has_more = has_more(&popular);
                        this.videos = popular.list.into_iter().map(VideoCard::from).collect();
                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Err(error) => this.fail(error, cx),
                }
                cx.notify();
            },
        );
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.loading_more || !self.has_more || self.more_error.is_some() {
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };

        let page = self.page + 1;
        let generation = self.generation;
        self.loading_more = true;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.popular(page, PAGE_SIZE).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_more = false;
                match result {
                    Ok(popular) => {
                        this.page = page;
                        this.has_more = has_more(&popular);
                        append_unique(
                            &mut this.videos,
                            popular.list.into_iter().map(VideoCard::from).collect(),
                        );
                    }
                    Err(error) => this.more_error = Some(api_error(error, cx)),
                }
                cx.notify();
            },
        );
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

fn has_more(popular: &Popular) -> bool {
    !popular.no_more && !popular.list.is_empty()
}

#[cfg(test)]
mod tests {
    use super::has_more;
    use gpuili_bilibili::Popular;

    #[test]
    fn stops_when_the_feed_is_empty_or_exhausted() {
        let empty = Popular {
            list: Vec::new(),
            no_more: false,
        };
        assert!(!has_more(&empty));

        let last = Popular {
            list: vec![popular_video("BV1")],
            no_more: true,
        };
        assert!(!has_more(&last));

        let more = Popular {
            list: vec![popular_video("BV1")],
            no_more: false,
        };
        assert!(has_more(&more));
    }

    fn popular_video(bvid: &str) -> gpuili_bilibili::PopularVideo {
        gpuili_bilibili::PopularVideo {
            aid: 0,
            bvid: bvid.into(),
            cid: 0,
            pic: String::new(),
            title: String::new(),
            duration: 0,
            pubdate: 0,
            owner: gpuili_bilibili::Owner {
                mid: 0,
                name: String::new(),
                face: String::new(),
            },
            stat: gpuili_bilibili::Stat::default(),
        }
    }
}
