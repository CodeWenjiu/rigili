use std::collections::HashSet;

use gpui_kit::*;
use gpuili_bilibili::{ApiError, Popular, PopularVideo};

use super::HomeView;
use crate::backend::Backend;
use crate::platform::runtime::Tokio;

const PAGE_SIZE: u32 = 30;

impl HomeView {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        if self.backend.is_none() {
            match Backend::new() {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    self.loading = false;
                    self.error = Some(error.to_string());
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

        let task = Tokio::spawn(&*cx, async move { backend.popular(1, PAGE_SIZE).await });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(Ok(popular)) => {
                        this.page = 1;
                        this.has_more = has_more(&popular);
                        this.videos = popular.list;
                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Ok(Err(error)) => this.fail(&error, cx),
                    Err(error) => this.error = Some(format!("加载任务失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
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

        let task = Tokio::spawn(&*cx, async move { backend.popular(page, PAGE_SIZE).await });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_more = false;
                match result {
                    Ok(Ok(popular)) => {
                        this.page = page;
                        this.has_more = has_more(&popular);
                        append_unique(&mut this.videos, popular.list);
                    }
                    Ok(Err(error)) => this.more_error = Some(api_error(&error, cx)),
                    Err(error) => this.more_error = Some(format!("加载任务失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
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

    fn fail(&mut self, error: &ApiError, cx: &mut Context<Self>) {
        self.error = Some(api_error(error, cx));
    }
}

fn api_error(error: &ApiError, cx: &mut App) -> String {
    if error.is_login_required() {
        crate::login::expire(cx);
    }
    crate::format::api_error_message(error)
}

fn has_more(popular: &Popular) -> bool {
    !popular.no_more && !popular.list.is_empty()
}

fn append_unique(videos: &mut Vec<PopularVideo>, incoming: Vec<PopularVideo>) {
    let mut seen = videos
        .iter()
        .map(|video| video.bvid.clone())
        .collect::<HashSet<_>>();
    for video in incoming {
        if seen.insert(video.bvid.clone()) {
            videos.push(video);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{append_unique, has_more};
    use gpuili_bilibili::{Owner, Popular, PopularVideo, Stat};

    fn video(bvid: &str) -> PopularVideo {
        PopularVideo {
            aid: 0,
            bvid: bvid.into(),
            cid: 0,
            pic: String::new(),
            title: String::new(),
            duration: 0,
            pubdate: 0,
            owner: Owner {
                mid: 0,
                name: String::new(),
                face: String::new(),
            },
            stat: Stat::default(),
        }
    }

    #[test]
    fn stops_when_the_feed_is_empty_or_exhausted() {
        let empty = Popular {
            list: Vec::new(),
            no_more: false,
        };
        assert!(!has_more(&empty));

        let last = Popular {
            list: vec![video("BV1")],
            no_more: true,
        };
        assert!(!has_more(&last));

        let more = Popular {
            list: vec![video("BV1")],
            no_more: false,
        };
        assert!(has_more(&more));
    }

    #[test]
    fn appends_only_new_videos() {
        let mut videos = vec![video("BV1")];
        append_unique(&mut videos, vec![video("BV1"), video("BV2")]);
        assert_eq!(
            videos
                .iter()
                .map(|video| video.bvid.as_str())
                .collect::<Vec<_>>(),
            ["BV1", "BV2"]
        );
    }
}
