rigili_macro::mod_prv!(input, native, poll, render);

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::slider::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Chapter, VideoShot};
use rigili_player::{PlaybackPhase, Player, PlayerStatus};

use super::controls::VolumePopup;
use crate::request::{Streams, VideoQuality};
use native::{NativeSurface, create_native_surface};

const PREVIEW_WIDTH: f32 = 200.;

// Video overlays stay light-on-dark regardless of the app theme so they stay
// readable over arbitrary frames.
const OVERLAY_SCRIM: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 0.,
    a: 0.5,
};
const OVERLAY_SCRIM_STRONG: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 0.,
    a: 0.62,
};
const OVERLAY_FOREGROUND: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 1.,
    a: 1.,
};

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const PAUSED_POLL_INTERVAL: Duration = Duration::from_millis(50);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONTROLS_IDLE: Duration = Duration::from_secs(3);
const SPEED_NOTICE: Duration = Duration::from_millis(1200);
const VOLUME_NOTICE: Duration = Duration::from_millis(900);
const SPEEDS: [f64; 10] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0];
const WHEEL_VOLUME_STEP: f32 = 5.0;
const PRECISE_VOLUME_STEP: f32 = 2.0;
const HOLD_SPEED: f64 = 4.0;
const HOLD_TAP_MAX: Duration = Duration::from_millis(250);
const SEEK_STEP: f64 = 5.0;

#[derive(Clone, Copy)]
enum Notice {
    Speed(f64),
    Volume(f64),
}

struct SpeedHold {
    previous: f64,
    started: Instant,
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
    speed_popover_open: bool,
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
    speed_hold: Option<SpeedHold>,
    notice: Option<(Notice, Instant)>,
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

        let mut subscriptions = vec![cx.subscribe_in(
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
        subscriptions.push(cx.on_blur(&focus, window, |this, _, cx| this.cancel_speed_hold(cx)));

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
            speed_popover_open: false,
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
            speed_hold: None,
            notice: None,
            _progress_sub: progress_sub,
            _subscriptions: subscriptions,
            _poll_task: poll_task,
        }
    }

    pub(super) fn load_streams(&mut self, streams: Streams, live: bool, cx: &mut Context<Self>) {
        if let Some(hold) = self.speed_hold.take() {
            self.status.speed = hold.previous;
            self.player.set_speed(hold.previous);
        }
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
        self.speed_popover_open = false;
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

    pub(super) fn reload_streams(&mut self, streams: Streams, cx: &mut Context<Self>) {
        let start = if self.status.phase == PlaybackPhase::Ended {
            0.
        } else {
            self.seeking.unwrap_or(self.status.time_pos).max(0.)
        };
        let paused = self.status.paused;
        if let Some(hold) = self.speed_hold.take() {
            self.status.speed = hold.previous;
            self.player.set_speed(hold.previous);
        }
        self.seeking = None;
        self.notice = None;
        self.quality_popover_open = false;
        self.speed_popover_open = false;
        self.selected_quality = streams.video_id;
        self.audio_url = streams.audio.clone();
        self.qualities = streams.qualities;
        self.status.phase = PlaybackPhase::Loading;
        self.status.error = None;
        self.player
            .switch_video(&streams.video, streams.audio.as_deref(), start, paused);
        self.last_activity = Instant::now();
        cx.notify();
    }

    pub(super) fn selected_quality(&self) -> u32 {
        self.selected_quality
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
        self.speed_popover_open = false;
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

    pub(super) fn set_speed_popover_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.speed_popover_open != open {
            self.speed_popover_open = open;
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

    pub(super) fn select_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        self.speed_popover_open = false;
        if (self.status.speed - speed).abs() < 1e-3 {
            cx.notify();
            return;
        }
        self.show_speed_notice(speed);
        self.apply_speed(speed, cx);
    }

    pub(super) fn begin_speed_hold(&mut self, cx: &mut Context<Self>) {
        if self.live || self.status.duration <= 0. || self.speed_hold.is_some() {
            return;
        }
        self.speed_hold = Some(SpeedHold {
            previous: self.status.speed,
            started: Instant::now(),
        });
        self.apply_speed(HOLD_SPEED, cx);
    }

    pub(super) fn end_speed_hold(&mut self, cx: &mut Context<Self>) {
        let Some(hold) = self.speed_hold.take() else {
            return;
        };
        self.apply_speed(hold.previous, cx);
        if hold.started.elapsed() < HOLD_TAP_MAX {
            self.seek_by(SEEK_STEP, cx);
        }
    }

    fn cancel_speed_hold(&mut self, cx: &mut Context<Self>) {
        if let Some(hold) = self.speed_hold.take() {
            self.apply_speed(hold.previous, cx);
        }
    }

    fn apply_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        self.status.speed = speed;
        self.player.set_speed(speed);
        self.last_activity = Instant::now();
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
            self.last_activity = Instant::now();
            self.show_speed_notice(speed);
            self.apply_speed(speed, cx);
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
}
