use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::slider::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use image::RgbaImage;
use rigili_bilibili::{Chapter, VideoShot, normalize_url};
use rigili_player::{PlaybackPhase, Player, PlayerStatus, VideoFrame, VideoSize};

use super::controls::{Transport, VolumePopup};
use rigili_core::{cursor, format_clock};

const PREVIEW_WIDTH: f32 = 200.;

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const CONTROLS_IDLE: Duration = Duration::from_secs(3);
const SPEEDS: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

pub(super) struct PlayerView {
    player: Player,
    progress: Entity<SliderState>,
    volume: Entity<SliderState>,
    volume_popup: Entity<VolumePopup>,
    frame: Option<Arc<RenderImage>>,
    retired_frames: Vec<Arc<RenderImage>>,
    status: PlayerStatus,
    seeking: Option<f64>,
    volume_dragging: bool,
    progress_max: f64,
    live: bool,
    controls_visible: bool,
    pointer_in_bar: bool,
    volume_popup_open: bool,
    last_activity: Instant,
    title: SharedString,
    chapters: Vec<Chapter>,
    previews: Option<VideoShot>,
    preview_time: Option<f64>,
    preview_hovered: bool,
    progress_bounds: Option<Bounds<Pixels>>,
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
        let volume_popup = cx.new(|cx| VolumePopup::new(volume.clone(), cx));

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
            volume_popup,
            frame: None,
            retired_frames: Vec::new(),
            status: PlayerStatus::default(),
            seeking: None,
            volume_dragging: false,
            progress_max: 0.0,
            live: false,
            controls_visible: false,
            pointer_in_bar: false,
            volume_popup_open: false,
            last_activity: Instant::now(),
            title: SharedString::default(),
            chapters: Vec::new(),
            previews: None,
            preview_time: None,
            preview_hovered: false,
            progress_bounds: None,
            _progress_sub: progress_sub,
            _subscriptions: subscriptions,
            _poll_task: poll_task,
        }
    }

    pub(super) fn load_urls(
        &mut self,
        video: String,
        audio: Option<String>,
        live: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(frame) = self.frame.take() {
            self.retired_frames.push(frame);
        }
        self.live = live;
        self.seeking = None;
        self.chapters.clear();
        self.previews = None;
        self.preview_time = None;
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

    pub(super) fn set_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        if self.title != title {
            self.title = title;
            cx.notify();
        }
    }

    pub(super) fn set_previews(&mut self, previews: VideoShot, cx: &mut Context<Self>) {
        self.previews = Some(previews);
        cx.notify();
    }

    pub(super) fn set_chapters(&mut self, chapters: Vec<Chapter>, cx: &mut Context<Self>) {
        self.chapters = chapters;
        cx.notify();
    }

    pub(super) fn set_progress_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.progress_bounds = Some(bounds);
    }

    pub(super) fn preview_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.progress_bounds else {
            return;
        };
        let width = f32::from(bounds.size.width);
        if width <= 0. || self.previews.is_none() || self.status.duration <= 0. {
            return;
        }
        let fraction = (f32::from(position.x - bounds.left()) / width).clamp(0., 1.);
        self.preview_time = Some(f64::from(fraction) * self.status.duration);
        cx.notify();
    }

    pub(super) fn preview_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.preview_hovered != hovered {
            self.preview_hovered = hovered;
            cx.notify();
        }
        if !hovered && self.seeking.is_none() && self.preview_time.take().is_some() {
            cx.notify();
        }
    }

    fn progress_preview(&self, cx: &App) -> Option<AnyElement> {
        let previews = self.previews.as_ref()?;
        let time = self.preview_time?;
        let frame = previews.frame_at(time)?;
        let bounds = self.progress_bounds?;
        let width = f32::from(bounds.size.width);
        if width <= 0. || frame.width == 0 || frame.height == 0 || self.status.duration <= 0. {
            return None;
        }

        let fraction = (time / self.status.duration).clamp(0., 1.);
        let scale = PREVIEW_WIDTH / frame.width as f32;
        let card_width = PREVIEW_WIDTH;
        let left =
            (fraction as f32 * width - card_width / 2.).clamp(0., (width - card_width).max(0.));
        let title = self
            .chapters
            .iter()
            .find(|chapter| time >= chapter.start && time < chapter.end)
            .map(|chapter| chapter.title.as_str())
            .filter(|title| !title.is_empty())
            .unwrap_or(self.title.as_str());
        let theme = cx.theme();

        Some(
            v_flex()
                .absolute()
                .left(px(left))
                .bottom(relative(1.))
                .mb_2()
                .w(px(card_width))
                .overflow_hidden()
                .popover_style(cx)
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(frame.height as f32 * scale))
                        .overflow_hidden()
                        .child(
                            img(normalize_url(frame.image))
                                .absolute()
                                .left(px(-(frame.x as f32) * scale))
                                .top(px(-(frame.y as f32) * scale))
                                .w(px(frame.sprite_width as f32 * scale))
                                .h(px(frame.sprite_height as f32 * scale)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left_1()
                                .bottom_1()
                                .px_1()
                                .rounded(theme.radius)
                                .bg(hsla(0., 0., 0., 0.65))
                                .text_xs()
                                .text_color(Hsla::white())
                                .child(format_clock(time)),
                        ),
                )
                .when(!title.is_empty(), |this| {
                    this.child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .truncate()
                            .child(title.to_owned()),
                    )
                })
                .into_any_element(),
        )
    }

    fn pointer_moved(&mut self, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        if !self.controls_visible {
            self.controls_visible = true;
            cx.notify();
        }
    }

    fn pointer_left_video(&mut self, cx: &mut Context<Self>) {
        self.pointer_in_bar = false;
        if self.controls_visible && !self.bar_interacting() {
            self.controls_visible = false;
            self.preview_time = None;
            cx.notify();
        }
    }

    fn bar_hovered(&mut self, hovered: bool) {
        self.pointer_in_bar = hovered;
        if hovered {
            self.last_activity = Instant::now();
        }
    }

    pub(super) fn set_volume_popup_open(&mut self, open: bool) {
        self.volume_popup_open = open;
        if open {
            self.last_activity = Instant::now();
        }
    }

    fn bar_interacting(&self) -> bool {
        self.pointer_in_bar
            || self.seeking.is_some()
            || self.volume_dragging
            || self.volume_popup_open
    }

    fn controls_idle(&self) -> bool {
        self.controls_visible
            && !self.bar_interacting()
            && self.preview_time.is_none()
            && self.last_activity.elapsed() >= CONTROLS_IDLE
    }

    fn on_progress(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        match event {
            SliderEvent::Change(SliderValue::Single(value)) => {
                self.seeking = Some(*value as f64);
                self.preview_time = self.previews.is_some().then_some(*value as f64);
                cx.notify();
            }
            SliderEvent::Release(SliderValue::Single(value)) => {
                self.seeking = None;
                self.status.time_pos = *value as f64;
                self.player.seek(*value as f64);
                if !self.preview_hovered {
                    self.preview_time = None;
                }
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_volume(&mut self, event: &SliderEvent, _: &mut Context<Self>) {
        self.last_activity = Instant::now();
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

        if self.controls_idle() {
            self.controls_visible = false;
            self.preview_time = None;
            cursor::hide_until_mouse_moves(cx);
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
        let (background, foreground, border, danger, radius) = {
            let theme = cx.theme();
            (
                theme.background,
                theme.foreground,
                theme.border,
                theme.danger,
                theme.radius_lg,
            )
        };
        let buffering = matches!(
            self.status.phase,
            PlaybackPhase::Buffering | PlaybackPhase::Loading
        );
        let frame = self.frame.clone();
        let player = self.player.clone();
        let controls = self.controls_visible.then(|| {
            div()
                .id("player-controls")
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .on_hover(cx.listener(|this, hovered, _, _| this.bar_hovered(*hovered)))
                .child(
                    Transport {
                        status: &self.status,
                        seeking: self.seeking,
                        progress: &self.progress,
                        progress_active: self.preview_hovered || self.seeking.is_some(),
                        chapters: &self.chapters,
                        volume_popup: self.volume_popup.clone(),
                        preview: self.progress_preview(cx),
                        live: self.live,
                        player: cx.entity().downgrade(),
                    }
                    .render(window, cx),
                )
        });

        let video = div()
            .id("player-video-surface")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(Hsla::black())
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| this.pointer_moved(cx)))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if *hovered {
                    this.pointer_moved(cx);
                } else {
                    this.pointer_left_video(cx);
                }
            }))
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
            )
            .when_some(controls, |this, controls| this.child(controls))
            .when(buffering, |this| {
                this.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Spinner::new().large()),
                )
            })
            .when_some(self.status.error.clone(), |this, error| {
                this.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .max_w_96()
                                .p_4()
                                .rounded(radius)
                                .border_1()
                                .border_color(border)
                                .bg(background)
                                .text_sm()
                                .text_color(danger)
                                .child(error),
                        ),
                )
            });

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(video)
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

    #[test]
    fn cycles_playback_speeds() {
        assert_eq!(next_speed(0.5), 0.75);
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(2.0), 0.5);
        assert_eq!(next_speed(9.0), 0.5);
    }
}
