use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::PopularVideo;

use crate::{Backend, runtime::Tokio};

const COVER_VARIANT: &str = "@480w_270h_1c.webp";

pub struct HomeView {
    backend: Option<Backend>,
    videos: Vec<PopularVideo>,
    loading: bool,
    error: Option<String>,
}

impl HomeView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (backend, error) = match Backend::new() {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let mut view = Self {
            backend,
            videos: Vec::new(),
            loading: false,
            error,
        };
        if view.backend.is_some() {
            view.load(cx);
        }
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
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
        self.loading = true;
        self.error = None;
        cx.notify();

        let task = Tokio::spawn(&*cx, async move { backend.popular(1, 30).await });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(Ok(popular)) => this.videos = popular.list,
                    Ok(Err(error)) => this.error = Some(error.to_string()),
                    Err(error) => this.error = Some(format!("加载任务失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for HomeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let muted_bg = cx.theme().muted;
        let foreground = cx.theme().foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let weak = cx.entity().downgrade();

        let content = if self.loading {
            div()
                .text_sm()
                .text_color(muted)
                .child("加载中…")
                .into_any_element()
        } else if let Some(error) = self.error.clone() {
            v_flex()
                .gap_2()
                .child(div().text_sm().text_color(danger).child(error))
                .child(Button::new("retry").primary().label("重试").on_click({
                    let weak = weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| this.load(cx));
                    }
                }))
                .into_any_element()
        } else if self.videos.is_empty() {
            div()
                .text_sm()
                .text_color(muted)
                .child("暂无内容")
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap_4()
                .children(
                    self.videos
                        .iter()
                        .map(|video| video_card(video, muted_bg, muted, foreground, radius)),
                )
                .into_any_element()
        };

        v_flex()
            .size_full()
            .gap_4()
            .p_4()
            .bg(cx.theme().background)
            .text_color(foreground)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("热门"),
                    )
                    .child(Button::new("refresh").ghost().label("刷新").on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.load(cx));
                        }
                    })),
            )
            .child(
                div()
                    .id("home-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

fn video_card(
    video: &PopularVideo,
    placeholder: Hsla,
    muted: Hsla,
    foreground: Hsla,
    radius: Pixels,
) -> impl IntoElement {
    v_flex()
        .w(px(240.))
        .gap_2()
        .child(
            div()
                .w_full()
                .h(px(135.))
                .rounded(radius)
                .overflow_hidden()
                .bg(placeholder)
                .child(
                    img(cover_url(&video.pic))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(foreground)
                .max_h(px(40.))
                .overflow_hidden()
                .child(video.title.clone()),
        )
        .child(
            h_flex()
                .justify_between()
                .text_xs()
                .text_color(muted)
                .child(video.owner.name.clone())
                .child(format_count(video.stat.view)),
        )
}

fn cover_url(url: &str) -> SharedString {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let base = rest.split('@').next().unwrap_or(rest);
    format!("https://{base}{COVER_VARIANT}").into()
}

fn format_count(value: u64) -> String {
    if value >= 100_000_000 {
        format!("{:.1} 亿", value as f64 / 100_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1} 万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{cover_url, format_count};

    #[test]
    fn formats_view_counts() {
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(12_345), "1.2 万");
        assert_eq!(format_count(123_456_789), "1.2 亿");
    }

    #[test]
    fn normalizes_cover_urls() {
        assert_eq!(
            cover_url("http://i0.hdslb.com/cover.jpg"),
            "https://i0.hdslb.com/cover.jpg@480w_270h_1c.webp"
        );
        assert_eq!(
            cover_url("https://i0.hdslb.com/cover.jpg@320w_200h_1c.webp"),
            "https://i0.hdslb.com/cover.jpg@480w_270h_1c.webp"
        );
    }
}
