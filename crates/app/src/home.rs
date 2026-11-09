use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::PopularVideo;

use crate::{
    Backend, cover_url, format_count, format_date, format_duration, runtime::Tokio,
    theme::cover_scrim,
};

pub enum HomeEvent {
    Open(String),
}

pub struct HomeView {
    backend: Option<Backend>,
    videos: Vec<PopularVideo>,
    loading: bool,
    error: Option<String>,
}

impl EventEmitter<HomeEvent> for HomeView {}

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
        let primary = cx.theme().primary;
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
                .children(self.videos.iter().map(|video| {
                    let bvid = video.bvid.clone();
                    let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(HomeEvent::Open(bvid.clone()));
                    });
                    video_card(video, foreground, muted, muted_bg, primary, radius, on_open)
                }))
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
    primary: Hsla,
    radius: Pixels,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let meta = format!("{} · {}", video.owner.name, format_date(video.pubdate));

    v_flex()
        .group("video-card")
        .w(px(240.))
        .gap_2()
        .id(SharedString::from(video.bvid.clone()))
        .cursor_pointer()
        .on_click(on_open)
        .child(
            div()
                .w_full()
                .aspect_ratio(16. / 9.)
                .relative()
                .rounded(radius)
                .overflow_hidden()
                .bg(placeholder)
                .child(
                    img(cover_url(&video.pic, 480))
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
                .group_hover("video-card", |style| style.text_color(primary))
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
