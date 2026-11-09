use std::{sync::Arc, time::Duration};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_player::{Player, PlayerStatus, VideoFrame, VideoSize};
use image::RgbaImage;

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const SPEEDS: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

pub struct PlayerView {
    player: Player,
    video_url: Entity<InputState>,
    audio_url: Entity<InputState>,
    progress: Entity<SliderState>,
    volume: Entity<SliderState>,
    frame: Option<Arc<RenderImage>>,
    retired_frames: Vec<Arc<RenderImage>>,
    status: PlayerStatus,
    seeking: Option<f64>,
    volume_dragging: bool,
    progress_max: f64,
    _progress_sub: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
    _poll_task: Task<()>,
}

impl PlayerView {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        initial_video: Option<String>,
        initial_audio: Option<String>,
    ) -> Self {
        let video_url = cx.new(|cx| {
            InputState::new(window, cx).placeholder("视频地址、B 站 DASH 视频流，或本地文件路径")
        });
        let audio_url =
            cx.new(|cx| InputState::new(window, cx).placeholder("可选：B 站 DASH 音频流地址"));
        let progress = cx.new(|_| SliderState::new().min(0.0).max(1.0).step(0.1));
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(100.0)
        });

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&video_url, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.load(cx);
            }
        }));
        subscriptions.push(cx.subscribe(&audio_url, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.load(cx);
            }
        }));
        subscriptions.push(cx.subscribe(&volume, |this, _, event: &SliderEvent, cx| {
            this.on_volume(event, cx);
        }));

        let progress_sub = Some(cx.subscribe(&progress, |this, _, event: &SliderEvent, cx| {
            this.on_progress(event, cx);
        }));

        let poll_task = cx.spawn_in(window, async move |weak, async_cx| {
            loop {
                async_cx.background_executor().timer(POLL_INTERVAL).await;
                if weak
                    .update_in(async_cx, |this, window, cx| this.pump(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });

        let mut view = Self {
            player: Player::new(),
            video_url,
            audio_url,
            progress,
            volume,
            frame: None,
            retired_frames: Vec::new(),
            status: PlayerStatus::default(),
            seeking: None,
            volume_dragging: false,
            progress_max: 0.0,
            _progress_sub: progress_sub,
            _subscriptions: subscriptions,
            _poll_task: poll_task,
        };

        if let Some(video) = initial_video {
            view.video_url
                .update(cx, |state, cx| state.set_value(video.clone(), window, cx));
            if let Some(audio) = initial_audio.as_ref() {
                view.audio_url.update(cx, |state, cx| {
                    state.set_value(audio.clone(), window, cx);
                });
            }
            view.status.loading = true;
            view.player.load(&video, initial_audio.as_deref());
        }

        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let video = self.video_url.read(cx).value().trim().to_string();
        if video.is_empty() {
            return;
        }
        let audio = self.audio_url.read(cx).value().trim().to_string();
        let audio = (!audio.is_empty()).then_some(audio);

        if let Some(frame) = self.frame.take() {
            self.retired_frames.push(frame);
        }
        self.seeking = None;
        self.progress_max = 0.0;
        self.status = PlayerStatus {
            loading: true,
            volume: self.status.volume,
            speed: self.status.speed,
            ..PlayerStatus::default()
        };
        self.player.load(&video, audio.as_deref());
        cx.notify();
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.player.toggle_pause();
        self.status.paused = !self.status.paused;
        cx.notify();
    }

    fn cycle_speed(&mut self, cx: &mut Context<Self>) {
        let speed = next_speed(self.status.speed);
        self.status.speed = speed;
        self.player.set_speed(speed);
        cx.notify();
    }

    fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.toggle_fullscreen();
        cx.notify();
    }

    fn on_progress(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(SliderValue::Single(value)) => {
                self.seeking = Some(*value as f64);
                cx.notify();
            }
            SliderEvent::Release(SliderValue::Single(value)) => {
                self.seeking = None;
                self.status.time_pos = *value as f64;
                self.player.seek(*value as f64);
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_volume(&mut self, event: &SliderEvent, _: &mut Context<Self>) {
        match event {
            SliderEvent::Change(SliderValue::Single(value)) => {
                self.volume_dragging = true;
                self.status.volume = *value as f64;
                self.player.set_volume(*value as f64);
            }
            SliderEvent::Release(_) => {
                self.volume_dragging = false;
            }
            _ => {}
        }
    }

    fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for frame in self.retired_frames.drain(..) {
            let _ = window.drop_image(frame);
        }

        let mut changed = false;
        if let Some(frame) = self.player.take_frame()
            && let Some(image) = to_render_image(frame)
        {
            if let Some(previous) = self.frame.replace(image) {
                let _ = window.drop_image(previous);
            }
            changed = true;
        }

        let status = self.player.status();
        if status != self.status {
            if status.duration > 0.0 && (status.duration - self.progress_max).abs() > 0.5 {
                self.rebuild_progress(status.duration, status.time_pos, cx);
            }
            if self.seeking.is_none() {
                self.progress.update(cx, |state, cx| {
                    state.set_value(status.time_pos as f32, window, cx);
                });
            }
            if !self.volume_dragging {
                self.volume.update(cx, |state, cx| {
                    state.set_value(status.volume as f32, window, cx);
                });
            }
            self.status = status;
            changed = true;
        }

        if changed {
            cx.notify();
        }
    }

    fn rebuild_progress(&mut self, duration: f64, time_pos: f64, cx: &mut Context<Self>) {
        self.progress_max = duration;
        let max = duration.max(1.0) as f32;
        let value = time_pos.clamp(0.0, duration) as f32;
        self.progress = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(max)
                .step(0.1)
                .default_value(value)
        });
        self._progress_sub = Some(
            cx.subscribe(&self.progress, |this, _, event: &SliderEvent, cx| {
                this.on_progress(event, cx)
            }),
        );
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let border = cx.theme().border;
        let weak = cx.entity().downgrade();

        v_flex()
            .flex_shrink_0()
            .gap_2()
            .px_3()
            .py_3()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("播放器"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("libmpv · GPUI RenderImage"),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(Input::new(&self.video_url).flex_1())
                    .child(Button::new("load").primary().label("播放").on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.load(cx));
                        }
                    })),
            )
            .child(Input::new(&self.audio_url).flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("B 站 DASH 需要分别填入视频流与音频流地址；本地文件只需视频地址。"),
            )
            .children(
                self.status
                    .error
                    .clone()
                    .map(|error| div().text_sm().text_color(danger).child(error)),
            )
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let resolution = self
            .status
            .video_size
            .map(|size| format!("{}×{}", size.width, size.height))
            .unwrap_or_else(|| "—".into());
        let decoder = decoder_label(&self.status.hwdec);
        let state = if self.status.loading {
            "加载中"
        } else if self.status.buffering {
            "缓冲中"
        } else if self.status.ended {
            "播放结束"
        } else if self.status.idle {
            "未播放"
        } else if self.status.paused {
            "已暂停"
        } else {
            "播放中"
        };

        let mut details = vec![resolution, decoder];
        if let Some(codec) = &self.status.audio_codec {
            details.push(format!("音频 {codec}"));
        }
        details.push(state.to_string());

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(muted)
            .child(details.join(" · "))
            .when(self.status.error.is_some(), |this| {
                this.child(div().text_color(danger).child("播放错误"))
            })
    }

    fn controls(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let weak = cx.entity().downgrade();
        let display_time = self.seeking.unwrap_or(self.status.time_pos);
        let duration = self.status.duration.max(0.0);
        let paused = self.status.paused;
        let speed = self.status.speed;
        let fullscreen = window.is_fullscreen();

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(border)
            .child(
                Button::new("toggle-pause")
                    .ghost()
                    .icon(if paused {
                        IconName::Play
                    } else {
                        IconName::Pause
                    })
                    .accessibility_label(if paused { "播放" } else { "暂停" })
                    .tooltip(if paused { "播放" } else { "暂停" })
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.toggle_pause(cx));
                        }
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format_time(display_time)),
            )
            .child(
                Slider::new(&self.progress)
                    .flex_1()
                    .accessibility_label("播放进度"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format_time(duration)),
            )
            .child(div().text_sm().text_color(muted).child("音量"))
            .child(
                Slider::new(&self.volume)
                    .w(px(110.0))
                    .accessibility_label("音量"),
            )
            .child(
                Button::new("speed")
                    .ghost()
                    .label(speed_label(speed))
                    .tooltip("切换倍速")
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.cycle_speed(cx));
                        }
                    }),
            )
            .child(
                Button::new("fullscreen")
                    .ghost()
                    .icon(if fullscreen {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .accessibility_label(if fullscreen { "退出全屏" } else { "全屏" })
                    .tooltip(if fullscreen { "退出全屏" } else { "全屏" })
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.toggle_fullscreen(window, cx);
                            });
                        }
                    }),
            )
    }
}

impl Render for PlayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (background, foreground) = {
            let theme = cx.theme();
            (theme.background, theme.foreground)
        };
        let fullscreen = window.is_fullscreen();
        let frame = self.frame.clone();
        let player = self.player.clone();

        let video = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(background)
            .children(frame.map(|frame| img(frame).size_full().object_fit(ObjectFit::Contain)))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let scale = window.scale_factor();
                        player.set_viewport(VideoSize::new(
                            (f32::from(bounds.size.width) * scale).round().max(0.0) as u32,
                            (f32::from(bounds.size.height) * scale).round().max(0.0) as u32,
                        ));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .when(!fullscreen, |this| this.child(self.header(cx)))
            .child(video)
            .child(self.status_bar(cx))
            .child(self.controls(window, cx))
    }
}

fn to_render_image(frame: VideoFrame) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(frame.width, frame.height, frame.data)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

fn next_speed(current: f64) -> f64 {
    SPEEDS
        .iter()
        .copied()
        .find(|speed| *speed > current + 1e-3)
        .unwrap_or(SPEEDS[0])
}

fn speed_label(speed: f64) -> String {
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}x")
}

fn decoder_label(hwdec: &Option<String>) -> String {
    match hwdec.as_deref() {
        Some("no") => "软件解码".into(),
        Some(value) if !value.is_empty() => format!("硬件解码 {value}"),
        _ => "解码未知".into(),
    }
}

fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".into();
    }

    let total = seconds.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::{decoder_label, format_time, next_speed, speed_label};

    #[test]
    fn formats_durations() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.4), "1:05");
        assert_eq!(format_time(3661.0), "1:01:01");
        assert_eq!(format_time(f64::NAN), "0:00");
        assert_eq!(format_time(-3.0), "0:00");
    }

    #[test]
    fn cycles_playback_speeds() {
        assert_eq!(next_speed(0.5), 0.75);
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(2.0), 0.5);
        assert_eq!(next_speed(9.0), 0.5);
    }

    #[test]
    fn labels_speeds_and_decoders() {
        assert_eq!(speed_label(1.0), "1x");
        assert_eq!(speed_label(1.25), "1.25x");
        assert_eq!(decoder_label(&Some("vaapi".into())), "硬件解码 vaapi");
        assert_eq!(decoder_label(&Some("no".into())), "软件解码");
        assert_eq!(decoder_label(&None), "解码未知");
    }
}
