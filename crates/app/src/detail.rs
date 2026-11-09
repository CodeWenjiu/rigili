use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::{Dash, VideoInfo};

use crate::{Backend, cover_url, format_count, format_date, format_duration, runtime::Tokio};

pub enum DetailEvent {
    Back,
    Play {
        video: String,
        audio: Option<String>,
    },
}

pub struct DetailView {
    bvid: Option<String>,
    backend: Option<Backend>,
    info: Option<VideoInfo>,
    loading: bool,
    error: Option<String>,
    playing: bool,
    focus: FocusHandle,
}

impl EventEmitter<DetailEvent> for DetailView {}

impl DetailView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (backend, error) = match Backend::new() {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            bvid: None,
            backend,
            info: None,
            loading: false,
            error,
            playing: false,
            focus: cx.focus_handle(),
        }
    }

    pub fn open(&mut self, bvid: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.bvid = Some(bvid.to_owned());
        self.info = None;
        self.error = None;
        self.playing = false;
        window.focus(&self.focus, cx);
        self.load_info(cx);
    }

    fn load_info(&mut self, cx: &mut Context<Self>) {
        let Some(bvid) = self.bvid.clone() else {
            return;
        };
        let Some(backend) = self.backend.clone() else {
            return;
        };

        self.loading = true;
        self.error = None;
        cx.notify();

        let task = Tokio::spawn(&*cx, async move { backend.view(&bvid).await });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(Ok(info)) => this.info = Some(info),
                    Ok(Err(error)) => this.error = Some(error.to_string()),
                    Err(error) => this.error = Some(format!("加载任务失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn play(&mut self, cx: &mut Context<Self>) {
        let Some(info) = self.info.clone() else {
            return;
        };
        let Some(backend) = self.backend.clone() else {
            return;
        };
        if self.playing {
            return;
        }

        self.playing = true;
        self.error = None;
        cx.notify();

        let task = Tokio::spawn(
            &*cx,
            async move { backend.playurl(&info.bvid, info.cid).await },
        );
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.playing = false;
                match result {
                    Ok(Ok(playurl)) => {
                        let Some(dash) = playurl.dash.as_ref() else {
                            this.error = Some("该视频没有 DASH 流，可能需要登录".into());
                            cx.notify();
                            return;
                        };
                        match dash_streams(dash) {
                            Some((video, audio)) => cx.emit(DetailEvent::Play { video, audio }),
                            None => this.error = Some("没有可用的视频流".into()),
                        }
                    }
                    Ok(Err(error)) => this.error = Some(error.to_string()),
                    Err(error) => this.error = Some(format!("加载任务失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn dash_streams(dash: &Dash) -> Option<(String, Option<String>)> {
    let video = dash.best_video()?;
    Some((
        video.base_url.clone(),
        dash.best_audio().map(|audio| audio.base_url.clone()),
    ))
}

impl Render for DetailView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let placeholder = cx.theme().muted;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let weak = cx.entity().downgrade();

        let content = if self.loading {
            v_flex()
                .gap_4()
                .child(Skeleton::new().h_8().w_2_3())
                .child(Skeleton::new().h_4().w_1_3())
                .child(Skeleton::new().h_64().w_full().rounded(radius))
                .into_any_element()
        } else if self.info.is_none() {
            v_flex()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(danger)
                        .child(self.error.clone().unwrap_or_else(|| "加载失败".into())),
                )
                .child(
                    Button::new("retry-detail")
                        .primary()
                        .label("重试")
                        .on_click({
                            let weak = weak.clone();
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| this.load_info(cx));
                            }
                        }),
                )
                .into_any_element()
        } else {
            let info = self.info.as_ref().expect("checked above");
            let title = info.title.clone();
            let owner = info.owner.name.clone();
            let meta = format!(
                "{} · {} · {}",
                format_date(info.pubdate),
                format_count(info.stat.view),
                format_duration(info.duration)
            );
            let desc = info.desc.clone();
            let cover = cover_url(&info.pic, 960);
            let bvid = info.bvid.clone();

            v_flex()
                .flex_1()
                .min_h_0()
                .gap_4()
                .child(
                    div()
                        .w_full()
                        .aspect_ratio(16. / 9.)
                        .max_h(px(420.))
                        .relative()
                        .rounded(radius)
                        .overflow_hidden()
                        .bg(placeholder)
                        .child(img(cover).size_full().object_fit(ObjectFit::Cover))
                        .child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .flex()
                                .justify_center()
                                .p_4()
                                .child(
                                    Button::new("play-detail")
                                        .primary()
                                        .large()
                                        .icon(if self.playing {
                                            IconName::LoaderCircle
                                        } else {
                                            IconName::Play
                                        })
                                        .label(if self.playing {
                                            "获取流…"
                                        } else {
                                            "播放"
                                        })
                                        .disabled(self.playing)
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| this.play(cx));
                                            }
                                        }),
                                ),
                        ),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_3()
                                .text_xs()
                                .text_color(muted)
                                .child(owner)
                                .child(meta)
                                .child(bvid),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .line_height(rems(1.6))
                        .text_color(muted)
                        .child(desc),
                )
                .children(
                    self.error
                        .clone()
                        .map(|error| div().text_sm().text_color(danger).child(error)),
                )
                .into_any_element()
        };

        v_flex()
            .id("detail-root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "enter" if this.info.is_some() => this.play(cx),
                    "escape" | "backspace" => cx.emit(DetailEvent::Back),
                    _ => {}
                }
            }))
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
                    .gap_2()
                    .child(
                        Button::new("back")
                            .ghost()
                            .icon(IconName::ArrowLeft)
                            .label("返回")
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |_, cx| cx.emit(DetailEvent::Back));
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(foreground)
                            .child("视频"),
                    ),
            )
            .child(
                div()
                    .id("detail-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}
