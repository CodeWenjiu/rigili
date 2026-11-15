use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::component::WindowExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Client, Danmaku, DanmakuMode};
use rigili_core::api_task_error_message;
use rigili_core::runtime::spawn_api;
use rigili_player::PlaybackPhase;

use crate::settings::DanmakuSettings;

use super::PlayerView;

const FIXED_SECONDS: f64 = 4.5;
const MAX_ACTIVE: usize = 120;
const MAX_SPAWN_PER_UPDATE: usize = 4;
const SPAWN_GRACE: f64 = 0.5;
const SEEK_BACK: f64 = 1.0;
const SEEK_FORWARD: f64 = 1.0;
const TIME_EPSILON: f64 = 0.001;
const DANMAKU_MAX_LEN: usize = 100;

pub(crate) struct DanmakuDemand {
    enabled: AtomicBool,
    playing: AtomicBool,
}

impl DanmakuDemand {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            playing: AtomicBool::new(false),
        }
    }

    pub(crate) fn should_load(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) && self.playing.load(Ordering::Relaxed)
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub(crate) fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(super) struct DanmakuSource {
    client: Arc<Client>,
    aid: u64,
    bvid: String,
    cid: u64,
}

#[derive(Default)]
pub(super) struct DanmakuState {
    items: Vec<Danmaku>,
    cursor: usize,
    last_time: Option<f64>,
    area: Option<Bounds<Pixels>>,
    scale: f32,
    lane_height: f32,
    lanes: Vec<f64>,
    active: Rc<RefCell<Vec<ActiveDanmaku>>>,
    settings: DanmakuSettings,
}

struct ActiveDanmaku {
    line: ShapedLine,
    line_height: Pixels,
    position: Point<Pixels>,
    width: f32,
    start: f64,
    expires: f64,
    scroll: bool,
    lane: usize,
    bottom: bool,
}

impl DanmakuState {
    fn with_settings(settings: DanmakuSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    pub(super) fn set_sorted(&mut self, items: Vec<Danmaku>) {
        self.items = items;
        self.cursor = 0;
        self.last_time = None;
        self.active.borrow_mut().clear();
        self.lanes.fill(0.);
    }

    pub(super) fn clear(&mut self) {
        self.items.clear();
        self.cursor = 0;
        self.last_time = None;
        self.active.borrow_mut().clear();
        self.lanes.fill(0.);
    }

    pub(super) fn seek(&mut self, time: f64) {
        self.reset_at(time);
    }

    pub(super) fn settings(&self) -> DanmakuSettings {
        self.settings
    }

    pub(super) fn active_count(&self) -> usize {
        self.active.borrow().len()
    }

    pub(super) fn update_settings(&mut self, settings: DanmakuSettings, time: f64) {
        if self.settings == settings {
            return;
        }
        self.settings = settings;
        self.active.borrow_mut().clear();
        self.apply_metrics();
        self.reset_at(time);
    }

    fn apply_metrics(&mut self) {
        let Some(area) = self.area else {
            return;
        };
        let height = f32::from(area.size.height);
        self.lane_height = (self.scale * self.settings.font_scale * 25. * 1.5).max(1.);
        let lanes = (height * self.settings.area_ratio / self.lane_height).floor() as usize;
        self.lanes = vec![0.; lanes.max(1)];
    }

    fn lane_y(&self, lane: usize, area: Bounds<Pixels>, bottom: bool) -> Pixels {
        if bottom {
            area.origin.y + px(f32::from(area.size.height) - (lane as f32 + 1.) * self.lane_height)
        } else {
            area.origin.y + px(lane as f32 * self.lane_height)
        }
    }

    fn layout_active(&mut self, active: &mut Vec<ActiveDanmaku>, time: f64, area: Bounds<Pixels>) {
        let count = self.lanes.len();
        active.retain(|item| item.lane < count);
        let width = f32::from(area.size.width);
        let speed = self.settings.scroll_seconds;

        for item in active.iter_mut() {
            let y = self.lane_y(item.lane, area, item.bottom);
            let x = if item.scroll {
                let elapsed = (time - item.start).max(0.) as f32;
                area.origin.x + px(width - elapsed / speed as f32 * (width + item.width))
            } else {
                area.origin.x + px((width - item.width) / 2.)
            };
            item.position = point(x, y);

            let free_at = if item.scroll {
                item.start + f64::from(item.width) * speed / f64::from(width + item.width) + 0.15
            } else {
                item.expires
            };
            if let Some(slot) = self.lanes.get_mut(item.lane) {
                *slot = slot.max(free_at);
            }
        }
    }

    pub(super) fn update(
        &mut self,
        time: f64,
        area: Bounds<Pixels>,
        scale: f32,
        window: &Window,
    ) -> bool {
        let width = f32::from(area.size.width);
        let height = f32::from(area.size.height);
        if width <= 0. || height <= 0. {
            return false;
        }

        let scale = scale.clamp(0.7, 1.6);
        let resized = self.area.is_none_or(|last| last != area);
        let rescaled = (self.scale - scale).abs() > f32::EPSILON;
        self.area = Some(area);

        let mut active = std::mem::take(&mut *self.active.borrow_mut());
        let mut changed = false;

        if resized || rescaled {
            self.scale = scale;
            self.apply_metrics();
            self.layout_active(&mut active, time, area);
            changed = true;
        }

        let previous = self.last_time;
        if let Some(last) = previous {
            let backlog = self
                .items
                .get(self.cursor)
                .is_some_and(|item| item.time <= time);
            if !resized && !rescaled && (time - last).abs() < TIME_EPSILON && !backlog {
                *self.active.borrow_mut() = active;
                return false;
            }
            if is_discontinuity(last, time) {
                rigili_monitor::monitor!("danmaku", "discontinuity {last:.3} -> {time:.3}");
                self.reset_at(time);
                active.clear();
                changed = true;
            }
        }
        self.last_time = Some(time);

        let mut spawned = 0;
        for item in active.iter_mut() {
            if item.scroll {
                let elapsed = (time - item.start).max(0.) as f32;
                let speed = self.settings.scroll_seconds as f32;
                let x = width - elapsed / speed * (width + item.width);
                item.position = point(area.origin.x + px(x), item.position.y);
            }
        }
        let before = active.len();
        active.retain(|item| time < item.expires);
        changed |= active.len() != before;
        if previous.is_some_and(|last| time > last) && active.iter().any(|item| item.scroll) {
            changed = true;
        }

        while let Some(item) = self.items.get(self.cursor) {
            if item.time > time {
                break;
            }
            self.cursor += 1;
            if item.time < time - SPAWN_GRACE
                || !matches!(
                    item.mode,
                    DanmakuMode::Scroll
                        | DanmakuMode::Reverse
                        | DanmakuMode::Top
                        | DanmakuMode::Bottom
                )
                || active.len() >= MAX_ACTIVE
                || !self.has_free_lane(time)
            {
                continue;
            }
            let item = item.clone();
            if self.spawn(&item, time, area, window, &mut active) {
                changed = true;
                spawned += 1;
                if spawned >= MAX_SPAWN_PER_UPDATE {
                    break;
                }
            }
        }

        *self.active.borrow_mut() = active;
        changed
    }

    pub(super) fn render(&self) -> impl IntoElement {
        let active = self.active.clone();
        let clip = self.area;
        canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let Some(clip) = clip else {
                    return;
                };
                window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                    let started = Instant::now();
                    let items = active.borrow();
                    let left = clip.origin.x;
                    let top = clip.origin.y;
                    let right = left + clip.size.width;
                    let bottom = top + clip.size.height;
                    for item in items.iter() {
                        if item.position.x + px(item.width) <= left
                            || item.position.x >= right
                            || item.position.y + item.line_height <= top
                            || item.position.y >= bottom
                        {
                            continue;
                        }
                        let _ = item.line.paint(
                            item.position,
                            item.line_height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                    let elapsed = started.elapsed();
                    let count = items.len();
                    drop(items);
                    if elapsed > Duration::from_millis(3) {
                        rigili_monitor::monitor!("danmaku", "paint {count} items in {elapsed:?}");
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    fn reset_at(&mut self, time: f64) {
        self.active.borrow_mut().clear();
        self.lanes.fill(0.);
        self.cursor = self.items.partition_point(|item| item.time < time);
        self.last_time = Some(time);
    }

    fn spawn(
        &mut self,
        item: &Danmaku,
        now: f64,
        area: Bounds<Pixels>,
        window: &Window,
        active: &mut Vec<ActiveDanmaku>,
    ) -> bool {
        let width = f32::from(area.size.width);
        let font_size =
            (item.font_size.max(18) as f32 * self.scale * self.settings.font_scale).clamp(14., 64.);
        let line_height = px(font_size * 1.4);
        let text: SharedString = item.text.clone().into();
        let run = TextRun {
            len: text.len(),
            font: window.text_style().font(),
            color: danmaku_color(item.color, self.settings.opacity),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(text, px(font_size), &[run], None);
        let text_width = f32::from(line.width());
        let scrolling = !matches!(item.mode, DanmakuMode::Top | DanmakuMode::Bottom);

        let free_for = if scrolling {
            f64::from(text_width) * self.settings.scroll_seconds / f64::from(width + text_width)
                + 0.15
        } else {
            FIXED_SECONDS + 0.15
        };
        let Some(lane) = self.take_lane(now, free_for, item.mode == DanmakuMode::Bottom) else {
            return false;
        };
        let bottom = item.mode == DanmakuMode::Bottom;
        let y = self.lane_y(lane, area, bottom);

        let x = match item.mode {
            DanmakuMode::Top | DanmakuMode::Bottom => area.origin.x + px((width - text_width) / 2.),
            _ => {
                let elapsed = (now - item.time).max(0.) as f32;
                let x =
                    width - elapsed / self.settings.scroll_seconds as f32 * (width + text_width);
                area.origin.x + px(x)
            }
        };

        active.push(ActiveDanmaku {
            line,
            line_height,
            position: point(x, y),
            width: text_width,
            start: item.time,
            expires: item.time
                + if scrolling {
                    self.settings.scroll_seconds
                } else {
                    FIXED_SECONDS
                },
            scroll: scrolling,
            lane,
            bottom,
        });
        true
    }

    fn has_free_lane(&self, now: f64) -> bool {
        self.lanes.iter().any(|free_at| *free_at <= now)
    }

    fn take_lane(&mut self, now: f64, free_for: f64, from_bottom: bool) -> Option<usize> {
        let count = self.lanes.len();
        let lane = if from_bottom {
            (0..count).rev().find(|lane| self.lanes[*lane] <= now)
        } else {
            (0..count).find(|lane| self.lanes[*lane] <= now)
        }?;
        self.lanes[lane] = now + free_for;
        Some(lane)
    }
}

pub(super) struct DanmakuLayer {
    state: DanmakuState,
}

impl DanmakuLayer {
    pub(super) fn new(settings: DanmakuSettings) -> Self {
        Self {
            state: DanmakuState::with_settings(settings),
        }
    }

    pub(super) fn update(
        &mut self,
        time: f64,
        area: Bounds<Pixels>,
        scale: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.settings.enabled {
            return;
        }
        if self.state.update(time, area, scale, window) {
            cx.notify();
        }
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.state.clear();
        cx.notify();
    }

    pub(super) fn set_sorted(&mut self, items: Vec<Danmaku>, cx: &mut Context<Self>) {
        self.state.set_sorted(items);
        cx.notify();
    }

    pub(super) fn seek(&mut self, time: f64, cx: &mut Context<Self>) {
        self.state.seek(time);
        cx.notify();
    }

    pub(super) fn settings(&self) -> DanmakuSettings {
        self.state.settings()
    }

    pub(super) fn active_count(&self) -> usize {
        self.state.active_count()
    }

    pub(super) fn set_settings(
        &mut self,
        settings: DanmakuSettings,
        time: f64,
        cx: &mut Context<Self>,
    ) {
        self.state.update_settings(settings, time);
        cx.notify();
    }
}

impl Render for DanmakuLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.state.settings.enabled {
            self.state.render().into_any_element()
        } else {
            div().absolute().size_full().into_any_element()
        }
    }
}

impl PlayerView {
    pub(crate) fn clear_danmaku(&mut self, cx: &mut Context<Self>) {
        self.danmaku_layer.update(cx, |layer, cx| layer.clear(cx));
    }

    pub(crate) fn set_danmaku(&mut self, items: Vec<Danmaku>, cx: &mut Context<Self>) {
        self.danmaku_layer
            .update(cx, |layer, cx| layer.set_sorted(items, cx));
    }

    pub(super) fn update_danmaku(
        &mut self,
        time: f64,
        area: Bounds<Pixels>,
        scale: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.danmaku_layer
            .update(cx, |layer, cx| layer.update(time, area, scale, window, cx));
    }

    pub(super) fn seek_danmaku(&mut self, time: f64, cx: &mut Context<Self>) {
        self.danmaku_layer
            .update(cx, |layer, cx| layer.seek(time, cx));
    }

    pub(crate) fn danmaku_demand(&self) -> Arc<DanmakuDemand> {
        self.danmaku_demand.clone()
    }

    pub(crate) fn danmaku_settings(&self, cx: &App) -> DanmakuSettings {
        self.danmaku_layer.read(cx).settings()
    }

    pub(crate) fn toggle_danmaku(&mut self, cx: &mut Context<Self>) {
        let mut settings = self.danmaku_settings(cx);
        settings.enabled = !settings.enabled;
        self.apply_danmaku_settings(settings, cx);
    }

    pub(crate) fn apply_danmaku_settings(
        &mut self,
        settings: DanmakuSettings,
        cx: &mut Context<Self>,
    ) {
        let time = self.playback_time();
        self.danmaku_demand.set_enabled(settings.enabled);
        self.danmaku_layer
            .update(cx, |layer, cx| layer.set_settings(settings, time, cx));
        crate::settings::update(cx, |persisted| persisted.danmaku = settings);
        self.last_activity = Instant::now();
        cx.notify();
    }

    pub(crate) fn set_danmaku_source(
        &mut self,
        client: Arc<Client>,
        aid: u64,
        bvid: String,
        cid: u64,
        cx: &mut Context<Self>,
    ) {
        self.danmaku_source = Some(DanmakuSource {
            client,
            aid,
            bvid,
            cid,
        });
        self.danmaku_sending = false;
        cx.notify();
    }

    pub(crate) fn clear_danmaku_source(&mut self, cx: &mut Context<Self>) {
        if self.danmaku_source.take().is_some() {
            self.danmaku_sending = false;
            cx.notify();
        }
    }

    pub(crate) fn set_danmaku_settings_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.danmaku_settings_open != open {
            self.danmaku_settings_open = open;
            if open {
                self.last_activity = Instant::now();
            }
            cx.notify();
        }
    }

    pub(super) fn on_danmaku_input(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => self.submit_danmaku(window, cx),
            InputEvent::Focus => {
                self.danmaku_input_focused = true;
                self.last_activity = Instant::now();
                cx.notify();
            }
            InputEvent::Blur => {
                self.danmaku_input_focused = false;
                self.last_activity = Instant::now();
                cx.notify();
            }
            InputEvent::Change => cx.notify(),
        }
    }

    pub(crate) fn submit_danmaku(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.danmaku_sending {
            return;
        }
        let Some(source) = self.danmaku_source.clone() else {
            window.push_notification(Notification::error("当前视频暂不支持发送弹幕"), cx);
            return;
        };
        let content = self
            .danmaku_input
            .read(cx)
            .value()
            .trim()
            .replace(['\n', '\r'], " ");
        if content.is_empty() {
            return;
        }
        if content.chars().count() > DANMAKU_MAX_LEN {
            window.push_notification(
                Notification::error(format!("弹幕不能超过 {DANMAKU_MAX_LEN} 字")),
                cx,
            );
            return;
        }

        let progress = self.playback_time();
        let input = self.danmaku_input.clone();
        let window_handle = window.window_handle();
        let aid = source.aid;
        let cid = source.cid;
        self.danmaku_sending = true;
        cx.notify();

        spawn_api(
            cx,
            async move {
                source
                    .client
                    .post_danmaku(cid, &source.bvid, &content, progress)
                    .await
            },
            move |this, result, cx| {
                if !this
                    .danmaku_source
                    .as_ref()
                    .is_some_and(|current| current.cid == cid && current.aid == aid)
                {
                    return;
                }
                this.danmaku_sending = false;
                match result {
                    Ok(()) => {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            input.update(cx, |state, cx| state.set_value("", window, cx));
                            window.push_notification(Notification::success("弹幕已发送"), cx);
                        });
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        });
                    }
                }
                cx.notify();
            },
        );
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
}

fn is_discontinuity(last: f64, time: f64) -> bool {
    time < last - SEEK_BACK || time > last + SEEK_FORWARD
}

fn danmaku_color(rgb: u32, opacity: f32) -> Hsla {
    Rgba {
        r: ((rgb >> 16) & 0xff) as f32 / 255.,
        g: ((rgb >> 8) & 0xff) as f32 / 255.,
        b: (rgb & 0xff) as f32 / 255.,
        a: opacity.clamp(0.05, 1.0),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::{DanmakuDemand, DanmakuSettings, DanmakuState, is_discontinuity};

    #[test]
    fn loads_only_while_playing_and_enabled() {
        let demand = DanmakuDemand::new(true);
        assert!(!demand.should_load());

        demand.set_playing(true);
        assert!(demand.should_load());

        demand.set_enabled(false);
        assert!(!demand.should_load());

        demand.set_enabled(true);
        demand.set_playing(false);
        assert!(!demand.should_load());
    }

    #[test]
    fn detects_only_real_seeks() {
        assert!(!is_discontinuity(10.0, 9.5));
        assert!(!is_discontinuity(10.0, 11.0));
        assert!(is_discontinuity(10.0, 8.0));
        assert!(is_discontinuity(10.0, 12.0));
    }

    #[test]
    fn applying_settings_resets_the_clock() {
        let mut state = DanmakuState::default();
        let settings = DanmakuSettings {
            font_scale: 1.5,
            ..state.settings()
        };
        state.update_settings(settings, 42.0);

        assert_eq!(state.settings(), settings);
        assert_eq!(state.last_time, Some(42.0));
        assert!(state.cursor <= state.items.len());
    }

    #[test]
    fn disabling_clears_the_active_layer() {
        let mut state = DanmakuState::default();
        let settings = DanmakuSettings {
            enabled: false,
            ..DanmakuSettings::default()
        };
        state.update_settings(settings, 5.0);
        assert!(state.active.borrow().is_empty());
        assert!(!state.settings().enabled);
    }
}
