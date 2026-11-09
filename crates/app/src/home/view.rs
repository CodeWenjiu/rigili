use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::PopularVideo;

use super::card::{CardStyle, skeleton_card, video_card};
use crate::backend::Backend;
use crate::platform::runtime::Tokio;

pub enum HomeEvent {
    Open {
        bvid: String,
        cid: Option<u64>,
        title: String,
    },
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
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let style = CardStyle::new(cx);
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
                    let cid = (video.cid != 0).then_some(video.cid);
                    let title = video.title.clone();
                    let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(HomeEvent::Open {
                            bvid: bvid.clone(),
                            cid,
                            title: title.clone(),
                        });
                    });
                    video_card(video, style, on_open)
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
