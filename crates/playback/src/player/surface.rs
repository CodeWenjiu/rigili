rigili_macro::mod_prv!(input, native, poll, render);

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::ActiveTheme;
use gpui_kit::component::slider::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Chapter, VideoPage, VideoShot};
use rigili_player::{OsdOverlay, PlaybackPhase, Player, PlayerStatus};

use super::controls::VolumePopup;
use super::danmaku;
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
pub(super) const OVERLAY_FOREGROUND: Hsla = Hsla {
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
    pub(super) video_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    qualities: Vec<VideoQuality>,
    selected_quality: u32,
    audio_url: Option<String>,
    quality_popover_open: bool,
    speed_popover_open: bool,
    pages: Vec<VideoPage>,
    selected_cid: Option<u64>,
    page_popover_open: bool,
    seeking: Option<f64>,
    volume_dragging: bool,
    progress_max: f64,
    live: bool,
    controls_visible: bool,
    pointer_in_video: bool,
    pointer_in_bar: bool,
    volume_popup_open: bool,
    pub(super) last_activity: Instant,
    title: SharedString,
    chapters: Vec<Chapter>,
    previews: Option<VideoShot>,
    preview_time: Option<f64>,
    preview_hovered: bool,
    progress_bounds: Option<Bounds<Pixels>>,
    focus: FocusHandle,
    speed_hold: Option<SpeedHold>,
    pub(super) danmaku_layer: danmaku::DanmakuLayer,
    pub(super) danmaku_input: Entity<gpui_kit::component::input::InputState>,
    pub(super) danmaku_source: Option<danmaku::DanmakuSource>,
    pub(super) danmaku_demand: Arc<danmaku::DanmakuDemand>,
    pub(super) danmaku_sending: bool,
    pub(super) danmaku_settings_open: bool,
    pub(super) danmaku_input_focused: bool,
    pub(super) danmaku_pin: Option<danmaku::DanmakuPin>,
    pub(super) danmaku_like_generation: u64,
    last_pump: Instant,
    play_anchor: Option<(f64, Instant)>,
    notice: Option<(Notice, Instant)>,
    _progress_sub: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
    _poll_task: Task<()>,
}

pub(super) enum PlayerEvent {
    SelectPage(u64),
}

impl EventEmitter<PlayerEvent> for PlayerView {}

impl PlayerView {
    pub(super) fn set_danmaku_overlay(&mut self, overlay: Option<OsdOverlay>) {
        self.player.set_osd(overlay);
    }
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let persisted = crate::settings::current(cx);
        let initial_volume = persisted.volume.unwrap_or(100.);
        let progress = cx.new(|_| SliderState::new().min(0.0).max(1.0).step(0.1));
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(initial_volume as f32)
        });

        let mut subscriptions = vec![cx.subscribe_in(
            &volume,
            window,
            |this, _, event: &SliderEvent, window, cx| this.on_volume(event, window, cx),
        )];
        let volume_popup = cx.new(|cx| VolumePopup::new(volume.clone(), cx));
        let font_family = cx.theme().font_family.clone();
        let danmaku_layer = danmaku::DanmakuLayer::new(persisted.danmaku, font_family);

        let progress_sub = Some(cx.subscribe_in(
            &progress,
            window,
            |this, _, event: &SliderEvent, window, cx| this.on_progress(event, window, cx),
        ));

        let danmaku_input = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx)
                .placeholder("发个友善的弹幕见证当下")
                .submit_on_enter(true)
        });
        subscriptions.push(cx.subscribe_in(
            &danmaku_input,
            window,
            |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                this.on_danmaku_input(event, window, cx)
            },
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

        let player = Player::new(rigili_core::fonts::osd_dir().as_deref());
        let speed = SPEEDS
            .iter()
            .copied()
            .find(|value| (value - persisted.speed).abs() < 1e-3)
            .unwrap_or(1.0);
        player.set_speed(speed);
        if let Some(volume) = persisted.volume {
            player.set_volume(volume);
        }
        if let Some(muted) = persisted.mute {
            player.set_muted(muted);
        }
        let status_source = player.status();
        let mut status = (*status_source).clone();
        status.speed = speed;
        if let Some(volume) = persisted.volume {
            status.volume = volume;
        }
        if let Some(muted) = persisted.mute {
            status.muted = muted;
        }
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
            pages: Vec::new(),
            selected_cid: None,
            page_popover_open: false,
            seeking: None,
            volume_dragging: false,
            progress_max: 0.0,
            live: false,
            controls_visible: false,
            pointer_in_video: false,
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
            danmaku_layer,
            danmaku_input,
            danmaku_source: None,
            danmaku_demand: Arc::new(danmaku::DanmakuDemand::new(persisted.danmaku.enabled)),
            danmaku_sending: false,
            danmaku_settings_open: false,
            danmaku_input_focused: false,
            danmaku_pin: None,
            danmaku_like_generation: 0,
            last_pump: Instant::now(),
            play_anchor: None,
            notice: None,
            _progress_sub: progress_sub,
            _subscriptions: subscriptions,
            _poll_task: poll_task,
        }
    }

    pub(super) fn focus_player(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub(super) fn status(&self) -> &PlayerStatus {
        &self.status
    }

    pub(super) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.danmaku_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }

    pub(super) fn load_streams(
        &mut self,
        streams: Streams,
        live: bool,
        start: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        self.load_streams_at(streams, live, start, false, cx);
    }

    pub(super) fn load_streams_at(
        &mut self,
        streams: Streams,
        live: bool,
        start: Option<f64>,
        paused: bool,
        cx: &mut Context<Self>,
    ) {
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
        self.page_popover_open = false;
        self.clear_danmaku(cx);
        self.danmaku_source = None;
        self.danmaku_sending = false;
        self.danmaku_settings_open = false;
        self.play_anchor = None;
        self.selected_quality = streams.video_id;
        self.audio_url = streams.audio.clone();
        self.qualities = streams.qualities;
        self.status = PlayerStatus {
            phase: PlaybackPhase::Loading,
            time_pos: start.unwrap_or_default(),
            paused,
            volume: self.status.volume,
            muted: self.status.muted,
            speed: self.status.speed,
            ..PlayerStatus::default()
        };
        self.player.switch_video(
            &streams.video,
            streams.audio.as_deref(),
            start.unwrap_or_default(),
            paused,
        );
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
        self.page_popover_open = false;
        self.play_anchor = Some((start, Instant::now()));
        self.seek_danmaku(start, cx);
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
        self.page_popover_open = false;
        crate::settings::update(cx, |settings| settings.preferred_quality = Some(id));
        self.play_anchor = Some((start, Instant::now()));
        self.seek_danmaku(start, cx);
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

    pub(super) fn set_pages(&mut self, pages: Vec<VideoPage>, cid: u64, cx: &mut Context<Self>) {
        self.pages = pages;
        self.selected_cid = Some(cid);
        self.page_popover_open = false;
        cx.notify();
    }

    pub(super) fn clear_pages(&mut self, cx: &mut Context<Self>) {
        if !self.pages.is_empty() || self.selected_cid.is_some() {
            self.pages.clear();
            self.selected_cid = None;
            self.page_popover_open = false;
            cx.notify();
        }
    }

    pub(super) fn set_selected_page(&mut self, cid: u64, cx: &mut Context<Self>) {
        if self.selected_cid != Some(cid) {
            self.selected_cid = Some(cid);
            cx.notify();
        }
    }

    pub(super) fn adjacent_page(&self, delta: isize) -> Option<u64> {
        let selected = self.selected_cid?;
        let index = self.pages.iter().position(|page| page.cid == selected)?;
        let index = index.checked_add_signed(delta)?;
        self.pages.get(index).map(|page| page.cid)
    }

    pub(super) fn request_page(&mut self, cid: u64, cx: &mut Context<Self>) {
        self.page_popover_open = false;
        if self.selected_cid != Some(cid) {
            cx.emit(PlayerEvent::SelectPage(cid));
        }
        cx.notify();
    }

    pub(super) fn set_page_popover_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.page_popover_open != open {
            self.page_popover_open = open;
            if open {
                self.last_activity = Instant::now();
            }
            cx.notify();
        }
    }

    pub(super) fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.anchor_clock();
        self.player.toggle_pause();
        self.status.paused = !self.status.paused;
        cx.notify();
    }

    pub(super) fn set_paused(&mut self, paused: bool, cx: &mut Context<Self>) {
        if self.status.paused != paused {
            self.toggle_pause(cx);
        }
    }

    /// Moves the playback handle into another tab's player, returning the
    /// previous one so the caller can keep it warm for that tab.
    pub(super) fn swap_player(&mut self, incoming: Player, cx: &mut Context<Self>) -> Player {
        if self.native_requested {
            self.player.set_native_output(false);
        }
        let outgoing = std::mem::replace(&mut self.player, incoming);
        if let Some(hold) = self.speed_hold.take() {
            self.status.speed = hold.previous;
        }
        let volume = self.status.volume;
        let muted = self.status.muted;
        let speed = self.status.speed;
        self.player.set_volume(volume);
        self.player.set_muted(muted);
        self.player.set_speed(speed);
        self.status_source = self.player.status();
        self.status = (*self.status_source).clone();
        self.status.volume = volume;
        self.status.muted = muted;
        self.status.speed = speed;
        if let Some(frame) = self.frame.take() {
            self.retired_frames.push(frame);
        }
        self.live = false;
        self.seeking = None;
        self.play_anchor = None;
        self.notice = None;
        self.previews = None;
        self.preview_time = None;
        self.preview_hovered = false;
        self.quality_popover_open = false;
        self.speed_popover_open = false;
        self.page_popover_open = false;
        self.native_bounds = None;
        self.native_requested = self.native.is_some() && !self.native_failed;
        if self.native_requested {
            self.player.set_native_output(true);
        }
        if let Some(native) = &mut self.native {
            if let Ok(surface) = native.composition.platform_surface() {
                let _ = surface.set_visible(false);
            }
            native.visible = false;
        }
        self.player.redraw();
        self.last_pump = Instant::now();
        cx.notify();
        outgoing
    }

    pub(super) fn adopt_streams(&mut self, streams: &Streams, cx: &mut Context<Self>) {
        self.qualities.clone_from(&streams.qualities);
        self.selected_quality = streams.video_id;
        self.audio_url.clone_from(&streams.audio);
        cx.notify();
    }

    pub(super) fn sync_streams(&self, streams: &mut Streams) {
        if self.qualities.is_empty() {
            return;
        }
        streams.video_id = self.selected_quality;
        streams.audio.clone_from(&self.audio_url);
        streams.qualities.clone_from(&self.qualities);
        if let Some(quality) = self
            .qualities
            .iter()
            .find(|quality| quality.id == self.selected_quality)
        {
            streams.video.clone_from(&quality.url);
        }
    }

    pub(super) fn playback_time(&self) -> f64 {
        let Some((position, anchor)) = self.play_anchor else {
            return self.status.time_pos;
        };
        let time = if self.status.phase == PlaybackPhase::Playing && !self.status.paused {
            position + anchor.elapsed().as_secs_f64() * self.status.speed
        } else {
            position
        };
        if self.status.duration > 0. {
            time.min(self.status.duration)
        } else {
            time
        }
    }

    pub(super) fn anchor_clock(&mut self) {
        self.play_anchor = Some((self.playback_time(), Instant::now()));
    }

    pub(super) fn select_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        if self.live {
            return;
        }
        self.speed_popover_open = false;
        if (self.status.speed - speed).abs() < 1e-3 {
            cx.notify();
            return;
        }
        self.show_speed_notice(speed);
        self.apply_speed(speed, true, cx);
    }

    pub(super) fn begin_speed_hold(&mut self, cx: &mut Context<Self>) {
        if self.live || self.status.duration <= 0. || self.speed_hold.is_some() {
            return;
        }
        self.speed_hold = Some(SpeedHold {
            previous: self.status.speed,
            started: Instant::now(),
        });
        self.apply_speed(HOLD_SPEED, false, cx);
    }

    pub(super) fn end_speed_hold(&mut self, cx: &mut Context<Self>) {
        let Some(hold) = self.speed_hold.take() else {
            return;
        };
        self.apply_speed(hold.previous, false, cx);
        if hold.started.elapsed() < HOLD_TAP_MAX {
            self.seek_by(SEEK_STEP, cx);
        }
    }

    fn cancel_speed_hold(&mut self, cx: &mut Context<Self>) {
        if let Some(hold) = self.speed_hold.take() {
            self.apply_speed(hold.previous, false, cx);
        }
    }

    fn apply_speed(&mut self, speed: f64, remember: bool, cx: &mut Context<Self>) {
        if self.live {
            return;
        }
        self.anchor_clock();
        self.status.speed = speed;
        self.player.set_speed(speed);
        if remember {
            crate::settings::update(cx, |settings| settings.speed = speed);
        }
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
        self.play_anchor = Some((target, Instant::now()));
        self.seek_danmaku(target, cx);
        self.player.seek(target);
        self.last_activity = Instant::now();
        cx.notify();
    }

    pub(super) fn adjust_speed(&mut self, faster: bool, cx: &mut Context<Self>) {
        if self.live {
            return;
        }
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
            self.apply_speed(speed, true, cx);
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
        crate::settings::update(cx, |settings| settings.mute = Some(muted));
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
