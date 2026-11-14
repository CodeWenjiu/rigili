use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::slider::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use image::RgbaImage;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle, RawWindowHandle};
use rigili_bilibili::{Chapter, VideoShot, normalize_url};
use rigili_player::{PlaybackPhase, Player, PlayerStatus, VideoFrame, VideoSize};

use super::controls::{Transport, VolumePopup, speed_label};
use super::native::WaylandOutput;
use super::{Streams, VideoQuality};
use rigili_core::{cursor, format_clock};

const PREVIEW_WIDTH: f32 = 200.;

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const PAUSED_POLL_INTERVAL: Duration = Duration::from_millis(50);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONTROLS_IDLE: Duration = Duration::from_secs(3);
const SPEED_NOTICE: Duration = Duration::from_millis(1200);
const VOLUME_NOTICE: Duration = Duration::from_millis(900);
const SPEEDS: [f64; 10] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0];
const WHEEL_VOLUME_STEP: f32 = 5.0;
const PRECISE_VOLUME_STEP: f32 = 2.0;

#[derive(Clone, Copy)]
enum Notice {
    Speed(f64),
    Volume(f64),
}

pub(super) struct PlayerView {
    player: Player,
    progress: Entity<SliderState>,
    volume: Entity<SliderState>,
    volume_popup: Entity<VolumePopup>,
    frame: Option<Arc<RenderImage>>,
    retired_frames: Vec<Arc<RenderImage>>,
    status: PlayerStatus,
    status_source: Arc<PlayerStatus>,
    native: Option<NativeSurface>,
    native_requested: bool,
    native_failed: bool,
    native_bounds: Option<Bounds<DevicePixels>>,
    video_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    qualities: Vec<VideoQuality>,
    selected_quality: u32,
    audio_url: Option<String>,
    quality_popover_open: bool,
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
    focus: FocusHandle,
    notice: Option<(Notice, Instant)>,
    _progress_sub: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
    _poll_task: Task<()>,
}

struct NativeSurface {
    composition: WindowCompositionSurface,
    output: WaylandOutput,
    visible: bool,
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

        let subscriptions = vec![cx.subscribe_in(
            &volume,
            window,
            |this, _, event: &SliderEvent, window, cx| this.on_volume(event, window, cx),
        )];
        let volume_popup = cx.new(|cx| VolumePopup::new(volume.clone(), cx));

        let progress_sub = Some(cx.subscribe_in(
            &progress,
            window,
            |this, _, event: &SliderEvent, window, cx| this.on_progress(event, window, cx),
        ));

        let focus = cx.focus_handle();
        let deferred_focus = focus.clone();
        window.defer(cx, move |window, cx| deferred_focus.focus(window, cx));

        let poll_task = cx.spawn_in(window, async move |weak, async_cx| {
            let mut interval = POLL_INTERVAL;
            loop {
                async_cx.background_executor().timer(interval).await;
                let next = weak.update_in(async_cx, |this, window, cx| {
                    this.pump(window, cx);
                    this.poll_interval()
                });
                match next {
                    Ok(next) => interval = next,
                    Err(_) => break,
                }
            }
        });

        let player = Player::new();
        let status_source = player.status();
        let status = (*status_source).clone();
        let native = create_native_surface(window);

        Self {
            player,
            progress,
            volume,
            volume_popup,
            frame: None,
            retired_frames: Vec::new(),
            status,
            status_source,
            native,
            native_requested: false,
            native_failed: false,
            native_bounds: None,
            video_bounds: Rc::new(Cell::new(None)),
            qualities: Vec::new(),
            selected_quality: 0,
            audio_url: None,
            quality_popover_open: false,
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
            focus,
            notice: None,
            _progress_sub: progress_sub,
            _subscriptions: subscriptions,
            _poll_task: poll_task,
        }
    }

    pub(super) fn load_streams(&mut self, streams: Streams, live: bool, cx: &mut Context<Self>) {
        if let Some(frame) = self.frame.take() {
            self.retired_frames.push(frame);
        }
        self.live = live;
        self.seeking = None;
        self.chapters.clear();
        self.previews = None;
        self.preview_time = None;
        self.notice = None;
        self.progress_max = 0.0;
        self.quality_popover_open = false;
        self.selected_quality = streams.video_id;
        self.audio_url = streams.audio.clone();
        self.qualities = streams.qualities;
        self.status = PlayerStatus {
            phase: PlaybackPhase::Loading,
            volume: self.status.volume,
            muted: self.status.muted,
            speed: self.status.speed,
            ..PlayerStatus::default()
        };
        self.player.load(&streams.video, streams.audio.as_deref());
        self.native_requested = self.native.is_some() && !self.native_failed;
        self.native_bounds = None;
        if self.native_requested {
            self.player.set_native_output(true);
            if let Some(native) = &mut self.native {
                if let Ok(surface) = native.composition.platform_surface() {
                    let _ = surface.set_visible(false);
                }
                native.visible = false;
            }
        }
        cx.notify();
    }

    pub(super) fn switch_quality(&mut self, id: u32, cx: &mut Context<Self>) {
        if self.live || id == self.selected_quality {
            return;
        }
        let Some(quality) = self
            .qualities
            .iter()
            .find(|quality| quality.id == id)
            .cloned()
        else {
            return;
        };
        let start = self.seeking.unwrap_or(self.status.time_pos).max(0.0);
        let paused = self.status.paused;
        self.selected_quality = id;
        self.quality_popover_open = false;
        self.status.phase = PlaybackPhase::Loading;
        self.status.error = None;
        self.player
            .switch_video(&quality.url, self.audio_url.as_deref(), start, paused);
        cx.notify();
    }

    pub(super) fn set_quality_popover_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.quality_popover_open != open {
            self.quality_popover_open = open;
            if open {
                self.last_activity = Instant::now();
            }
            cx.notify();
        }
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
        self.show_speed_notice(speed);
        cx.notify();
    }

    pub(super) fn seek_by(&mut self, delta: f64, cx: &mut Context<Self>) {
        if self.live || self.status.duration <= 0. {
            return;
        }
        let current = self.seeking.unwrap_or(self.status.time_pos);
        let target = (current + delta).clamp(0., self.status.duration);
        self.seeking = None;
        self.status.time_pos = target;
        self.player.seek(target);
        self.last_activity = Instant::now();
        cx.notify();
    }

    pub(super) fn adjust_speed(&mut self, faster: bool, cx: &mut Context<Self>) {
        let current = self.status.speed;
        let index = SPEEDS
            .iter()
            .position(|speed| (speed - current).abs() < 1e-3)
            .unwrap_or(SPEEDS.len() - 1);
        let next = if faster {
            (index + 1).min(SPEEDS.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        let speed = SPEEDS[next];
        if (speed - current).abs() > 1e-3 {
            self.status.speed = speed;
            self.player.set_speed(speed);
            self.last_activity = Instant::now();
            self.show_speed_notice(speed);
            cx.notify();
        }
    }

    fn show_speed_notice(&mut self, speed: f64) {
        self.notice = Some((Notice::Speed(speed), Instant::now() + SPEED_NOTICE));
    }

    fn show_volume_notice(&mut self, volume: f64) {
        self.notice = Some((Notice::Volume(volume), Instant::now() + VOLUME_NOTICE));
    }

    pub(super) fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let muted = !self.status.muted;
        self.status.muted = muted;
        self.player.set_muted(muted);
        self.last_activity = Instant::now();
        cx.notify();
    }

    fn set_volume(&mut self, volume: f64, window: &mut Window, cx: &mut Context<Self>) {
        self.status.volume = volume;
        if self.status.muted {
            self.status.muted = false;
            self.player.set_muted(false);
        }
        self.player.set_volume(volume);
        self.volume
            .update(cx, |slider, cx| slider.set_value(volume as f32, window, cx));
        cx.notify();
    }

    fn on_scroll(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.source != ScrollSource::Pointer {
            return;
        }
        let step = scroll_volume_step(event.delta, window.line_height().as_f32());
        if step == 0.0 {
            return;
        }
        let volume = (self.status.volume + step).clamp(0.0, 100.0);
        self.show_volume_notice(volume);
        self.last_activity = Instant::now();
        self.set_volume(volume, window, cx);
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
            || self.quality_popover_open
    }

    fn controls_idle(&self) -> bool {
        self.controls_visible
            && !self.bar_interacting()
            && self.preview_time.is_none()
            && self.last_activity.elapsed() >= CONTROLS_IDLE
    }

    fn on_progress(&mut self, event: &SliderEvent, window: &mut Window, cx: &mut Context<Self>) {
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
                self.focus.focus(window, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_volume(&mut self, event: &SliderEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        match event {
            SliderEvent::Change(SliderValue::Single(value)) => {
                self.volume_dragging = true;
                self.set_volume(*value as f64, window, cx);
            }
            SliderEvent::Release(_) => {
                self.volume_dragging = false;
                self.focus.focus(window, cx);
            }
            _ => {}
        }
    }

    fn poll_interval(&self) -> Duration {
        poll_interval(self.status.phase, self.status.paused)
    }

    fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for frame in self.retired_frames.drain(..) {
            let _ = window.drop_image(frame);
        }

        self.sync_native_bounds(window, cx);
        let mut changed = self.pump_native();
        if notice_expired(self.notice.map(|(_, deadline)| deadline), Instant::now()) {
            self.notice = None;
            changed = true;
        }
        if let Some(frame) = self.player.take_frame()
            && let Some(image) = to_render_image(frame)
        {
            if let Some(previous) = self.frame.replace(image) {
                let _ = window.drop_image(previous);
            }
            changed = true;
        }

        let status = self.player.status();
        if !Arc::ptr_eq(&status, &self.status_source) {
            let next = (*status).clone();
            if next.duration > 0.0 && (next.duration - self.progress_max).abs() > 0.5 {
                self.rebuild_progress(next.duration, next.time_pos, window, cx);
            }
            if self.seeking.is_none() {
                self.progress.update(cx, |state, cx| {
                    state.set_value(next.time_pos as f32, window, cx);
                });
            }
            if !self.volume_dragging {
                self.volume.update(cx, |state, cx| {
                    state.set_value(next.volume as f32, window, cx);
                });
            }
            self.status = next;
            self.status_source = status;
            changed = true;
        }

        if self.controls_idle() {
            self.controls_visible = false;
            self.preview_time = None;
            cursor::hide_until_mouse_moves(cx);
            changed = true;
        }

        self.sync_native_bounds(window, cx);

        if changed {
            cx.notify();
        }
    }

    fn pump_native(&mut self) -> bool {
        let Some(mut native) = self.native.take() else {
            return false;
        };
        let mut changed = false;
        let mut failure = None;

        if self.native_requested {
            match native.output.poll() {
                Ok(released) => {
                    for slot in released {
                        self.player.native_frame_released(slot);
                    }
                }
                Err(error) => failure = Some(error),
            }

            if failure.is_none() {
                let frames = self.player.take_native_frames();
                if let Some((frame, skipped)) = frames.split_last() {
                    for skipped in skipped {
                        self.player.native_frame_released(skipped.slot);
                    }
                    match native.output.present(frame) {
                        Ok(()) => {
                            if !native.visible {
                                let _ = native
                                    .composition
                                    .platform_surface()
                                    .map(|surface| surface.set_visible(true));
                                native.output.commit();
                                native.visible = true;
                                changed = true;
                            }
                        }
                        Err(error) => failure = Some(error),
                    }
                }
            }

            if failure.is_none() && self.player.native_output_failed() {
                failure = Some("the renderer cannot export GPU frames".into());
            }
        }

        if let Some(error) = failure {
            self.native_failed = true;
            self.native_requested = false;
            self.player.set_native_output(false);
            if let Ok(surface) = native.composition.platform_surface() {
                let _ = surface.set_visible(false);
            }
            native.visible = false;
            changed = true;
            eprintln!("direct GPU output unavailable, using readback: {error}");
        }

        self.native = Some(native);
        changed
    }

    fn sync_native_bounds(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.native_requested || self.native_failed {
            return;
        }
        let Some(area) = self.video_bounds.get() else {
            return;
        };
        let rect = contain_rect(area, self.status.video_size);
        let bounds = rect.to_device_pixels(window.scale_factor());
        if self.native_bounds == Some(bounds) {
            return;
        }
        self.native_bounds = Some(bounds);
        let Some(native) = &self.native else {
            return;
        };
        if let Ok(surface) = native.composition.platform_surface() {
            if surface.set_bounds(bounds).is_err() {
                self.native_failed = true;
                self.native_requested = false;
                self.player.set_native_output(false);
                let _ = surface.set_visible(false);
                return;
            }
            native.output.commit();
        }
        cx.notify();
    }

    fn rebuild_progress(
        &mut self,
        duration: f64,
        time_pos: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        self._progress_sub = Some(cx.subscribe_in(
            &self.progress,
            window,
            |this, _, event: &SliderEvent, window, cx| this.on_progress(event, window, cx),
        ));
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
        let notice = self.notice.map(|(notice, _)| match notice {
            Notice::Speed(speed) => speed_label(speed),
            Notice::Volume(volume) => format!("音量 {volume:.0}%"),
        });
        let paused = self.status.paused && !self.controls_visible;
        let native_active = self.native_requested && !self.native_failed && self.native.is_some();
        let frame = (!native_active).then(|| self.frame.clone()).flatten();
        let video_bounds = self.video_bounds.clone();
        let player = self.player.clone();

        let mut overlays: Vec<AnyElement> = Vec::new();
        if self.controls_visible {
            overlays.push(
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
                            qualities: &self.qualities,
                            selected_quality: self.selected_quality,
                            quality_open: self.quality_popover_open,
                            preview: self.progress_preview(cx),
                            live: self.live,
                            player: cx.entity().downgrade(),
                        }
                        .render(window, cx),
                    )
                    .into_any_element(),
            );
        }
        if buffering {
            overlays.push(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Spinner::new().large())
                    .into_any_element(),
            );
        }
        if let Some(error) = self.status.error.clone() {
            overlays.push(
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
                    )
                    .into_any_element(),
            );
        }
        if paused && !buffering && self.status.error.is_none() && notice.is_none() {
            overlays.push(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(80.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(hsla(0., 0., 0., 0.5))
                            .child(
                                Icon::new(IconName::Pause)
                                    .size(px(36.))
                                    .text_color(Hsla::white()),
                            ),
                    )
                    .into_any_element(),
            );
        }
        if let Some(label) = notice {
            overlays.push(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .rounded(radius)
                            .bg(hsla(0., 0., 0., 0.62))
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(Hsla::white())
                            .child(label),
                    )
                    .into_any_element(),
            );
        }
        let video = div()
            .id("player-video-surface")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(Hsla::black())
            .children(frame.map(|frame| img(frame).size_full().object_fit(ObjectFit::Contain)))
            .child(
                canvas(
                    move |bounds, window, _| {
                        video_bounds.set(Some(bounds));
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
            .child(
                div()
                    .id("player-video-focus")
                    .absolute()
                    .size_full()
                    .track_focus(&self.focus)
                    .key_context(crate::shortcuts::VIDEO),
            )
            .child(deferred(
                div()
                    .id("player-video-input")
                    .absolute()
                    .size_full()
                    .on_mouse_move(
                        cx.listener(|this, _: &MouseMoveEvent, _, cx| this.pointer_moved(cx)),
                    )
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        if *hovered {
                            this.pointer_moved(cx);
                        } else {
                            this.pointer_left_video(cx);
                        }
                    }))
                    .on_scroll(cx.listener(|this, event: &ScrollEvent, window, cx| {
                        this.on_scroll(event, window, cx);
                    }))
                    .children(overlays),
            ));

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(video)
    }
}

fn create_native_surface(window: &mut Window) -> Option<NativeSurface> {
    let surface = window
        .enable_window_composition()
        .ok()
        .and_then(|composition| composition.create_native_surface().ok())?;
    let handle = surface.platform_surface().ok()?.platform_handle().ok()?;
    let handle = handle.downcast::<RawWindowHandle>().ok()?;
    let RawWindowHandle::Wayland(surface_handle) = *handle else {
        return None;
    };
    let display = window.display_handle().ok()?;
    let RawDisplayHandle::Wayland(display_handle) = display.as_raw() else {
        return None;
    };
    let output = WaylandOutput::new(
        display_handle.display.as_ptr(),
        surface_handle.surface.as_ptr(),
    )
    .map_err(|error| eprintln!("direct GPU output unavailable: {error}"))
    .ok()?;
    Some(NativeSurface {
        composition: surface,
        output,
        visible: false,
    })
}

fn contain_rect(area: Bounds<Pixels>, source: Option<VideoSize>) -> Bounds<Pixels> {
    let Some(source) = source.filter(|size| size.width > 0 && size.height > 0) else {
        return area;
    };
    let area_width = f32::from(area.size.width);
    let area_height = f32::from(area.size.height);
    let scale = (area_width / source.width as f32).min(area_height / source.height as f32);
    let width = source.width as f32 * scale;
    let height = source.height as f32 * scale;
    Bounds::new(
        point(
            area.origin.x + px((area_width - width) / 2.0),
            area.origin.y + px((area_height - height) / 2.0),
        ),
        size(px(width), px(height)),
    )
}

fn precise_scroll(delta: ScrollDelta) -> bool {
    match delta {
        ScrollDelta::Pixels(_) => true,
        ScrollDelta::Lines(lines) => {
            (lines.y - lines.y.round()).abs() > 0.01 || (lines.x - lines.x.round()).abs() > 0.01
        }
    }
}

fn scroll_volume_step(delta: ScrollDelta, line_height: f32) -> f64 {
    let lines = match delta {
        ScrollDelta::Lines(delta) => delta.y,
        ScrollDelta::Pixels(delta) => delta.y.as_f32() / line_height.max(1.0),
    };
    f64::from(
        lines
            * if precise_scroll(delta) {
                PRECISE_VOLUME_STEP
            } else {
                WHEEL_VOLUME_STEP
            },
    )
}

fn notice_expired(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_some_and(|deadline| deadline <= now)
}

fn poll_interval(phase: PlaybackPhase, paused: bool) -> Duration {
    match phase {
        PlaybackPhase::Idle | PlaybackPhase::Ended => IDLE_POLL_INTERVAL,
        _ if paused => PAUSED_POLL_INTERVAL,
        _ => POLL_INTERVAL,
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
    use std::time::{Duration, Instant};

    use super::{
        IDLE_POLL_INTERVAL, PAUSED_POLL_INTERVAL, POLL_INTERVAL, next_speed, notice_expired,
        poll_interval, scroll_volume_step,
    };
    use gpui_kit::{ScrollDelta, point, px};
    use rigili_player::PlaybackPhase;

    #[test]
    fn maps_wheel_and_trackpad_scroll_to_volume() {
        assert_eq!(
            scroll_volume_step(ScrollDelta::Lines(point(0., 1.)), 20.0),
            5.0
        );
        assert_eq!(
            scroll_volume_step(ScrollDelta::Lines(point(0., -2.)), 20.0),
            -10.0
        );
        assert_eq!(
            scroll_volume_step(ScrollDelta::Pixels(point(px(0.), px(20.))), 20.0),
            2.0
        );
        assert_eq!(
            scroll_volume_step(ScrollDelta::Pixels(point(px(0.), px(-10.))), 20.0),
            -1.0
        );
        assert_eq!(
            scroll_volume_step(ScrollDelta::Lines(point(0.5, 0.)), 20.0),
            0.0
        );
    }

    #[test]
    fn cycles_playback_speeds() {
        assert_eq!(next_speed(0.5), 0.75);
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(2.0), 2.5);
        assert_eq!(next_speed(3.0), 3.5);
        assert_eq!(next_speed(4.0), 0.5);
        assert_eq!(next_speed(9.0), 0.5);
    }

    #[test]
    fn notices_expire_after_their_deadline() {
        let now = Instant::now();
        assert!(!notice_expired(None, now));
        assert!(!notice_expired(Some(now + Duration::from_secs(1)), now));
        assert!(notice_expired(Some(now), now));
    }

    #[test]
    fn polls_slowly_when_idle_or_paused() {
        assert_eq!(poll_interval(PlaybackPhase::Playing, false), POLL_INTERVAL);
        assert_eq!(
            poll_interval(PlaybackPhase::Buffering, false),
            POLL_INTERVAL
        );
        assert_eq!(
            poll_interval(PlaybackPhase::Playing, true),
            PAUSED_POLL_INTERVAL
        );
        assert_eq!(poll_interval(PlaybackPhase::Idle, true), IDLE_POLL_INTERVAL);
        assert_eq!(
            poll_interval(PlaybackPhase::Ended, false),
            IDLE_POLL_INTERVAL
        );
    }
}
