use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::PopularVideo;

use crate::{Backend, runtime::Tokio, theme::cover_scrim};

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
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let muted_bg = cx.theme().muted;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let weak = cx.entity().downgrade();

        let content = if self.loading {
            h_flex()
                .flex_wrap()
                .gap_x_4()
                .gap_y_6()
                .children((0..8).map(|_| skeleton_card(radius)))
                .into_any_element()
        } else if let Some(error) = self.error.clone() {
            v_flex()
                .gap_3()
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
                .gap_x_4()
                .gap_y_6()
                .children(
                    self.videos
                        .iter()
                        .map(|video| video_card(video, foreground, muted, muted_bg, radius)),
                )
                .into_any_element()
        };

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_5()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("热门"),
                    )
                    .child(
                        Button::new("refresh")
                            .ghost()
                            .icon(IconName::RefreshCw)
                            .label("刷新")
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| this.load(cx));
                                }
                            }),
                    ),
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
    foreground: Hsla,
    muted: Hsla,
    placeholder: Hsla,
    radius: Pixels,
) -> impl IntoElement {
    let meta = format!("{} · {}", video.owner.name, format_date(video.pubdate));

    v_flex()
        .w(px(240.))
        .gap_2()
        .child(
            div()
                .w_full()
                .aspect_ratio(16. / 9.)
                .relative()
                .rounded(radius)
                .overflow_hidden()
                .bg(placeholder)
                .child(
                    img(cover_url(&video.pic))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .child(
                    h_flex()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .items_center()
                        .gap_3()
                        .px_2()
                        .pt_4()
                        .pb_1()
                        .text_xs()
                        .text_color(Hsla::white())
                        .bg(cover_scrim())
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(IconName::Play)
                                .child(format_count(video.stat.view)),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(IconName::MessageCircle)
                                .child(format_count(video.stat.danmaku)),
                        )
                        .child(div().flex_1())
                        .child(format_duration(video.duration)),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_clamp(2)
                .text_color(foreground)
                .child(video.title.clone()),
        )
        .child(div().text_xs().text_color(muted).child(meta))
}

fn skeleton_card(radius: Pixels) -> impl IntoElement {
    v_flex()
        .w(px(240.))
        .gap_2()
        .child(
            Skeleton::new()
                .w_full()
                .h_auto()
                .aspect_ratio(16. / 9.)
                .rounded(radius),
        )
        .child(Skeleton::new().h_4().w_3_4())
        .child(Skeleton::new().h_3().w_1_3())
}

fn cover_url(url: &str) -> SharedString {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let base = rest.split('@').next().unwrap_or(rest);
    format!("https://{base}{COVER_VARIANT}").into()
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn format_date(unix: i64) -> String {
    let (_, month, day) = civil_from_days(unix.div_euclid(86_400));
    format!("{month}-{day}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

fn format_count(value: u64) -> String {
    if value >= 100_000_000 {
        format!("{:.1}亿", value as f64 / 100_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1}万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{cover_url, format_count, format_date, format_duration};

    #[test]
    fn formats_view_counts() {
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(12_345), "1.2万");
        assert_eq!(format_count(123_456_789), "1.2亿");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(156), "02:36");
        assert_eq!(format_duration(3_600), "1:00:00");
    }

    #[test]
    fn formats_publish_dates() {
        assert_eq!(format_date(1_762_646_400), "11-9");
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
