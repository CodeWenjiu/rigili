use std::{sync::Arc, time::Duration};

use gpui_kit::component::slider::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_player::{PlaybackPhase, Player, PlayerStatus, VideoFrame, VideoSize};
use image::RgbaImage;

use super::controls::{Transport, status_bar};

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const SPEEDS: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

pub(super) struct PlayerView {
    player: Player,
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
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let progress = cx.new(|_| SliderState::new().min(0.0).max(1.0).step(0.1));
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(100.0)
        });

        let subscriptions = vec![cx.subscribe(&volume, |this, _, event: &SliderEvent, cx| {
            this.on_volume(event, cx);
        })];

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

        Self {
            player: Player::new(),
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
        }
    }

    pub(super) fn load_urls(
        &mut self,
        video: String,
        audio: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(frame) = self.frame.take() {
            self.retired_frames.push(frame);
        }
        self.seeking = None;
        self.progress_max = 0.0;
        self.status = PlayerStatus {
            phase: PlaybackPhase::Loading,
            volume: self.status.volume,
            speed: self.status.speed,
            ..PlayerStatus::default()
        };
        self.player.load(&video, audio.as_deref());
        cx.notify();
    }

    pub(super) fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.player.toggle_pause();
        self.status.paused = !self.status.paused;
        cx.notify();
    }

    pub(super) fn cycle_speed(&mut self, cx: &mut Context<Self>) {
        let speed = next_speed(self.status.speed);
        self.status.speed = speed;
        self.player.set_speed(speed);
        cx.notify();
    }

    pub(super) fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            .child(video)
            .when(!fullscreen, |this| this.child(status_bar(&self.status, cx)))
            .child(
                Transport {
                    status: &self.status,
                    seeking: self.seeking,
                    progress: &self.progress,
                    volume: &self.volume,
                    player: cx.entity().downgrade(),
                }
                .render(window, cx),
            )
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

#[cfg(test)]
mod tests {
    use super::next_speed;
    use crate::format::format_clock;

    #[test]
    fn cycles_playback_speeds() {
        assert_eq!(next_speed(0.5), 0.75);
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(2.0), 0.5);
        assert_eq!(next_speed(9.0), 0.5);
    }

    #[test]
    fn formats_progress_without_padding_minutes() {
        assert_eq!(format_clock(9.0), "0:09");
    }
}
