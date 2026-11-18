use std::{fmt::Write as _, sync::Arc};

use gpui_kit::*;
use rigili_bilibili::{Danmaku, DanmakuMode};

use crate::settings::DanmakuSettings;

const FIXED_SECONDS: f64 = 4.5;
const ACTIVE_PER_LANE: usize = 4;
const MIN_ACTIVE: usize = 16;
const MAX_ACTIVE: usize = 120;
const MAX_SPAWN_PER_UPDATE: usize = 4;
const SPAWN_GRACE: f64 = 0.5;
const SEEK_BACK: f64 = 1.0;
const SEEK_FORWARD: f64 = 1.0;
const TIME_EPSILON: f64 = 0.001;

#[derive(Default)]
pub(super) struct DanmakuState {
    items: Arc<Vec<Danmaku>>,
    cursor: usize,
    last_time: Option<f64>,
    area: Option<(f32, f32)>,
    scale: f32,
    lane_height: f32,
    lanes: Vec<f64>,
    active: Vec<ActiveDanmaku>,
    settings: DanmakuSettings,
    font_family: SharedString,
    dirty: bool,
}

struct ActiveDanmaku {
    text: String,
    font_size: f32,
    color: u32,
    position: (f32, f32),
    width: f32,
    start: f64,
    expires: f64,
    scroll: bool,
    lane: usize,
    bottom: bool,
}

impl DanmakuState {
    pub(super) fn with_settings(settings: DanmakuSettings, font_family: SharedString) -> Self {
        Self {
            settings,
            font_family,
            ..Self::default()
        }
    }

    pub(super) fn set_sorted(&mut self, items: Arc<Vec<Danmaku>>) {
        self.items = items;
        let time = self.last_time.unwrap_or(0.);
        self.cursor = self.items.partition_point(|item| item.time < time);
        self.dirty = true;
    }

    pub(super) fn clear(&mut self) {
        self.items = Arc::new(Vec::new());
        self.cursor = 0;
        self.last_time = None;
        self.active.clear();
        self.lanes.fill(0.);
        self.dirty = true;
    }

    pub(super) fn seek(&mut self, time: f64) {
        self.reset_at(time);
        self.dirty = true;
    }

    fn active_limit(&self) -> usize {
        active_limit(self.lanes.len())
    }

    pub(super) fn settings(&self) -> DanmakuSettings {
        self.settings
    }

    pub(super) fn active_count(&self) -> usize {
        self.active.len()
    }

    pub(super) fn update_settings(&mut self, settings: DanmakuSettings, time: f64) {
        if self.settings == settings {
            return;
        }
        self.settings = settings;
        self.active.clear();
        self.apply_metrics();
        self.reset_at(time);
        self.dirty = true;
    }

    fn apply_metrics(&mut self) {
        let Some((_, height)) = self.area else {
            return;
        };
        self.lane_height = (self.scale * self.settings.font_scale * 25. * 1.5).max(1.);
        let lanes = (height * self.settings.area_ratio / self.lane_height).floor() as usize;
        self.lanes = vec![0.; lanes.max(1)];
    }

    fn lane_y(&self, lane: usize, bottom: bool) -> f32 {
        if bottom {
            let height = self.area.map_or(0., |(_, height)| height);
            height - (lane as f32 + 1.) * self.lane_height
        } else {
            lane as f32 * self.lane_height
        }
    }

    fn layout_active(&mut self, active: &mut Vec<ActiveDanmaku>, time: f64, width: f32) {
        let count = self.lanes.len();
        active.retain(|item| item.lane < count);
        let speed = self.settings.scroll_seconds;

        for item in active.iter_mut() {
            let y = self.lane_y(item.lane, item.bottom);
            let x = if item.scroll {
                let elapsed = (time - item.start).max(0.) as f32;
                width - elapsed / speed as f32 * (width + item.width)
            } else {
                (width - item.width) / 2.
            };
            item.position = (x, y);

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

    pub(super) fn update(&mut self, time: f64, width: f32, height: f32, scale: f32) -> bool {
        if width <= 0. || height <= 0. {
            return false;
        }

        let scale = scale.clamp(0.7, 1.6);
        let resized = self.area.is_none_or(|last| last != (width, height));
        let rescaled = (self.scale - scale).abs() > f32::EPSILON;
        self.area = Some((width, height));

        let mut active = std::mem::take(&mut self.active);
        let mut changed = std::mem::take(&mut self.dirty);

        if resized || rescaled {
            self.scale = scale;
            self.apply_metrics();
            self.layout_active(&mut active, time, width);
            changed = true;
        }

        let previous = self.last_time;
        if let Some(last) = previous {
            let backlog = self
                .items
                .get(self.cursor)
                .is_some_and(|item| item.time <= time);
            if !resized && !rescaled && (time - last).abs() < TIME_EPSILON && !backlog {
                self.active = active;
                return changed;
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
                item.position.0 = width - elapsed / speed * (width + item.width);
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
                || active.len() >= self.active_limit()
                || !self.has_free_lane(time)
            {
                continue;
            }
            let item = item.clone();
            if self.spawn(&item, time, width, &mut active) {
                changed = true;
                spawned += 1;
                if spawned >= MAX_SPAWN_PER_UPDATE {
                    break;
                }
            }
        }

        self.active = active;
        changed
    }

    pub(super) fn ass(&self, device_scale: f32) -> String {
        let alpha = ((1. - self.settings.opacity.clamp(0.05, 1.)) * 255.).round() as u8;
        let mut out = String::with_capacity(self.active.len() * 64);
        for item in &self.active {
            let red = ((item.color >> 16) & 0xff) as u8;
            let green = ((item.color >> 8) & 0xff) as u8;
            let blue = (item.color & 0xff) as u8;
            let font_size = item.font_size * device_scale;
            let _ = writeln!(
                out,
                "{{\\an7\\pos({:.1},{:.1})\\fn{}\\fs{font_size:.1}\\c&H{:02X}{:02X}{:02X}&\\3c&H000000&\\bord{:.1}\\shad0\\alpha&H{alpha:02X}&}}{}",
                item.position.0 * device_scale,
                item.position.1 * device_scale,
                self.font_family,
                blue,
                green,
                red,
                outline_width(font_size),
                escape_ass(&item.text)
            );
        }
        out
    }

    fn reset_at(&mut self, time: f64) {
        self.active.clear();
        self.lanes.fill(0.);
        self.cursor = self.items.partition_point(|item| item.time < time);
        self.last_time = Some(time);
    }

    fn spawn(
        &mut self,
        item: &Danmaku,
        now: f64,
        width: f32,
        active: &mut Vec<ActiveDanmaku>,
    ) -> bool {
        let font_size =
            (item.font_size.max(18) as f32 * self.scale * self.settings.font_scale).clamp(14., 64.);
        let text_width = estimate_width(&item.text, font_size);
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
        let y = self.lane_y(lane, bottom);

        let x = match item.mode {
            DanmakuMode::Top | DanmakuMode::Bottom => (width - text_width) / 2.,
            _ => {
                let elapsed = (now - item.time).max(0.) as f32;
                width - elapsed / self.settings.scroll_seconds as f32 * (width + text_width)
            }
        };

        active.push(ActiveDanmaku {
            text: item.text.clone(),
            font_size,
            color: item.color,
            position: (x, y),
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

fn is_discontinuity(last: f64, time: f64) -> bool {
    time < last - SEEK_BACK || time > last + SEEK_FORWARD
}

// Danmaku sit on arbitrary video frames, so the glyphs need their own dark edge
// to stay legible where the background happens to share their colour.
fn outline_width(font_size: f32) -> f32 {
    (font_size * 0.08).max(1.)
}

fn active_limit(lanes: usize) -> usize {
    (lanes * ACTIVE_PER_LANE).clamp(MIN_ACTIVE, MAX_ACTIVE)
}

fn escape_ass(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => {}
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

// libass needs a text width before anything is rendered, and shaping it here
// would mean carrying a text system into the player thread.
fn estimate_width(text: &str, font_size: f32) -> f32 {
    let units: f32 = text.chars().map(char_units).sum();
    units * font_size / 2.
}

fn char_units(ch: char) -> f32 {
    if is_wide(ch) { 2. } else { 1. }
}

fn is_wide(ch: char) -> bool {
    matches!(
        u32::from(ch),
        0x1100..=0x115F
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
            | 0x20000..=0x3FFFD
    )
}

#[cfg(test)]
mod tests {
    use super::{
        DanmakuState, MAX_ACTIVE, MIN_ACTIVE, active_limit, escape_ass, is_discontinuity,
        outline_width,
    };
    use crate::settings::DanmakuSettings;
    use rigili_bilibili::{Danmaku, DanmakuMode};
    use std::sync::Arc;

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
        assert!(state.active.is_empty());
        assert!(!state.settings.enabled);
    }

    #[test]
    fn caps_active_danmaku_by_display_area() {
        assert_eq!(active_limit(1), MIN_ACTIVE);
        assert_eq!(active_limit(6), 24);
        assert_eq!(active_limit(21), 84);
        assert_eq!(active_limit(128), MAX_ACTIVE);
    }

    #[test]
    fn keeps_the_cursor_when_items_are_replaced() {
        let item = |time| Danmaku {
            time,
            mode: DanmakuMode::Scroll,
            font_size: 25,
            color: 0x00ff_ffff,
            text: String::new(),
        };
        let mut state = DanmakuState {
            last_time: Some(5.0),
            ..DanmakuState::default()
        };
        state.set_sorted(Arc::new(vec![item(0.0), item(1.0), item(6.0), item(9.0)]));

        assert_eq!(state.cursor, 2);
        assert_eq!(state.active_count(), 0);
    }

    #[test]
    fn seeking_requests_one_redraw() {
        let mut state = DanmakuState {
            area: Some((1280., 720.)),
            scale: 1.,
            last_time: Some(3.),
            ..DanmakuState::default()
        };
        state.apply_metrics();
        state.seek(3.);

        assert!(state.update(3., 1280., 720., 1.));
        assert!(!state.update(3., 1280., 720., 1.));
    }

    #[test]
    fn writes_ass_events_with_position_and_colour() {
        let mut state = DanmakuState {
            area: Some((1280., 720.)),
            scale: 1.,
            ..DanmakuState::default()
        };
        state.apply_metrics();

        let mut active = Vec::new();
        let item = Danmaku {
            time: 1.0,
            mode: DanmakuMode::Scroll,
            font_size: 25,
            color: 0x00ff_0000,
            text: "hi".into(),
        };
        assert!(state.spawn(&item, 1.0, 1280., &mut active));
        state.active = active;

        let ass = state.ass(1.);
        assert!(ass.contains("\\an7"));
        assert!(ass.contains("\\pos("));
        assert!(ass.contains("\\c&H0000FF&"));
        assert!(ass.contains("\\3c&H000000&"));
        assert!(ass.contains("\\bord"));
        assert!(!ass.contains("\\bord0"));
        assert!(ass.ends_with("}hi\n"), "{ass}");
    }

    #[test]
    fn outlines_scale_with_the_rendered_font_size() {
        assert!((outline_width(25.) - 2.).abs() < 0.01);
        assert!((outline_width(50.) - 4.).abs() < 0.01);
        assert_eq!(outline_width(4.), 1.);
    }

    #[test]
    fn escapes_ass_override_braces() {
        assert_eq!(escape_ass("a{b}c\\d"), "a\\{b\\}cd");
    }
}
