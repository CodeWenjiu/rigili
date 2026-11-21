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
const FONT_SIZE_FACTOR: f32 = 0.7;
// Fingers are not precise, and a frozen danmaku is easier to grab than to miss.
const PIN_PADDING: f32 = 4.;
// A pinned danmaku carries the like affordance after its text, so the pointer
// may travel past the last glyph and still be considered on it.
const PIN_SLACK: f32 = 64.;

#[derive(Default)]
pub(super) struct DanmakuState {
    items: Arc<Vec<Danmaku>>,
    cursor: usize,
    last_time: Option<f64>,
    area: Option<(f32, f32)>,
    lane_height: f32,
    lanes: Vec<f64>,
    active: Vec<ActiveDanmaku>,
    settings: DanmakuSettings,
    font_family: SharedString,
    dirty: bool,
}

struct ActiveDanmaku {
    id: u64,
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
    frozen: Option<f64>,
}

pub(super) struct Pinned<'a> {
    pub(super) id: u64,
    pub(super) text: &'a str,
    pub(super) font_family: &'a SharedString,
    pub(super) color: u32,
    pub(super) font_size: f32,
    pub(super) position: (f32, f32),
    pub(super) width: f32,
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

    pub(super) fn pinned(&self) -> Option<Pinned<'_>> {
        let item = self.active.iter().find(|item| item.frozen.is_some())?;
        Some(Pinned {
            id: item.id,
            text: &item.text,
            font_family: &self.font_family,
            color: item.color,
            font_size: item.font_size,
            position: item.position,
            width: item.width,
        })
    }

    /// Freezes the topmost danmaku under the pointer, or releases the current
    /// one when the pointer is not over it. Returns whether the pin changed.
    pub(super) fn pin_at(&mut self, x: f32, y: f32, time: f64, hold: bool) -> bool {
        let mut changed = false;
        if let Some(item) = self.active.iter().find(|item| item.frozen.is_some()) {
            // A drag that leaves the text must keep it frozen, or the selection
            // would be cut off mid-gesture.
            if pinned_hit(item, x, y) || hold {
                return false;
            }
            changed = self.unpin(time);
        }

        let Some(item) = self.active.iter_mut().rev().find(|item| hit(item, x, y)) else {
            return changed;
        };
        item.frozen = Some(time);
        // The lane stays blocked while the danmaku hangs in place, otherwise a
        // new comment would fly straight through it.
        if let Some(slot) = self.lanes.get_mut(item.lane) {
            *slot = f64::INFINITY;
        }
        self.dirty = true;
        true
    }

    pub(super) fn unpin(&mut self, time: f64) -> bool {
        let speed = self.settings.scroll_seconds;
        let width = self.area.map_or(0., |(width, _)| width);
        let Some(item) = self.active.iter_mut().find(|item| item.frozen.is_some()) else {
            return false;
        };
        let Some(frozen_at) = item.frozen.take() else {
            return false;
        };
        let shift = (time - frozen_at).max(0.);
        item.start += shift;
        item.expires += shift;
        let free_at = lane_free_at(item, speed, width);
        if let Some(slot) = self.lanes.get_mut(item.lane) {
            *slot = free_at;
        }
        self.dirty = true;
        true
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
        self.lane_height = (FONT_SIZE_FACTOR * self.settings.font_scale * 25. * 1.5).max(1.);
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

    fn font_size(&self, base: u8) -> f32 {
        (f32::from(base.max(18)) * FONT_SIZE_FACTOR * self.settings.font_scale).clamp(14., 64.)
    }

    fn layout_active(&mut self, active: &mut Vec<ActiveDanmaku>, time: f64, width: f32) {
        let count = self.lanes.len();
        active.retain(|item| item.lane < count);
        let speed = self.settings.scroll_seconds;

        for item in active.iter_mut() {
            if item.frozen.is_some() {
                continue;
            }
            let y = self.lane_y(item.lane, item.bottom);
            let x = if item.scroll {
                let elapsed = (time - item.start).max(0.) as f32;
                width - elapsed / speed as f32 * (width + item.width)
            } else {
                (width - item.width) / 2.
            };
            item.position = (x, y);

            let free_at = self.lane_free_at(item);
            if let Some(slot) = self.lanes.get_mut(item.lane) {
                *slot = slot.max(free_at);
            }
        }
    }

    fn lane_free_at(&self, item: &ActiveDanmaku) -> f64 {
        let width = self.area.map_or(0., |(width, _)| width);
        lane_free_at(item, self.settings.scroll_seconds, width)
    }

    pub(super) fn update(&mut self, time: f64, width: f32, height: f32) -> bool {
        if width <= 0. || height <= 0. {
            return false;
        }

        let resized = self.area.is_none_or(|last| last != (width, height));
        self.area = Some((width, height));

        let mut active = std::mem::take(&mut self.active);
        let mut changed = std::mem::take(&mut self.dirty);

        if resized {
            for item in active.iter_mut() {
                item.frozen = None;
            }
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
            if !resized && (time - last).abs() < TIME_EPSILON && !backlog {
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
            if item.scroll && item.frozen.is_none() {
                let elapsed = (time - item.start).max(0.) as f32;
                let speed = self.settings.scroll_seconds as f32;
                item.position.0 = width - elapsed / speed * (width + item.width);
            }
        }
        let before = active.len();
        active.retain(|item| item.frozen.is_some() || time < item.expires);
        changed |= active.len() != before;
        if previous.is_some_and(|last| time > last)
            && active
                .iter()
                .any(|item| item.scroll && item.frozen.is_none())
        {
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
        let em_scale = rigili_core::fonts::libass_em_scale();
        let mut out = String::with_capacity(self.active.len() * 64);
        for item in self.active.iter().filter(|item| item.frozen.is_none()) {
            let red = ((item.color >> 16) & 0xff) as u8;
            let green = ((item.color >> 8) & 0xff) as u8;
            let blue = (item.color & 0xff) as u8;
            let font_size = item.font_size * device_scale * em_scale;
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
        let font_size = self.font_size(item.font_size);
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
            id: item.id,
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
            frozen: None,
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

fn lane_free_at(item: &ActiveDanmaku, speed: f64, width: f32) -> f64 {
    if item.scroll {
        item.start + f64::from(item.width) * speed / f64::from(width + item.width) + 0.15
    } else {
        item.expires
    }
}

fn hit(item: &ActiveDanmaku, x: f32, y: f32) -> bool {
    let height = item.font_size * 1.5;
    let (left, top) = item.position;
    x >= left - PIN_PADDING
        && x <= left + item.width + PIN_PADDING
        && y >= top - PIN_PADDING
        && y <= top + height + PIN_PADDING
}

fn pinned_hit(item: &ActiveDanmaku, x: f32, y: f32) -> bool {
    let height = item.font_size * 1.5;
    let (left, top) = item.position;
    x >= left - PIN_PADDING
        && x <= left + item.width + PIN_SLACK
        && y >= top - PIN_PADDING
        && y <= top + height + PIN_PADDING
}

// Danmaku sit on arbitrary video frames, so the glyphs need their own dark edge
// to stay legible where the background happens to share their colour.
fn outline_width(font_size: f32) -> f32 {
    (font_size * 0.04).max(1.)
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
            id: 0,
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
            last_time: Some(3.),
            ..DanmakuState::default()
        };
        state.apply_metrics();
        state.seek(3.);

        assert!(state.update(3., 1280., 720.));
        assert!(!state.update(3., 1280., 720.));
    }

    #[test]
    fn font_size_only_follows_the_danmaku_settings() {
        let mut state = DanmakuState::default();
        assert!((state.font_size(25) - 17.5).abs() < 0.01);
        assert!((state.font_size(18) - 14.).abs() < 0.01);
        assert!((state.font_size(4) - 14.).abs() < 0.01);

        state.settings.font_scale = 1.5;
        assert!((state.font_size(25) - 26.25).abs() < 0.01);
        assert!((state.font_size(64) - 64.).abs() < 0.01);
    }

    #[test]
    fn writes_ass_events_with_position_and_colour() {
        let mut state = DanmakuState {
            area: Some((1280., 720.)),
            ..DanmakuState::default()
        };
        state.apply_metrics();

        let mut active = Vec::new();
        let item = Danmaku {
            id: 2,
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
        assert!(
            ass.contains(&format!(
                "\\fs{:.1}",
                25. * super::FONT_SIZE_FACTOR * rigili_core::fonts::libass_em_scale()
            )),
            "{ass}"
        );
        assert!(ass.contains("\\bord"));
        assert!(!ass.contains("\\bord0"));
        assert!(ass.ends_with("}hi\n"), "{ass}");
    }

    #[test]
    fn outlines_scale_with_the_rendered_font_size() {
        assert!((outline_width(25.) - 1.).abs() < 0.01);
        assert!((outline_width(50.) - 2.).abs() < 0.01);
        assert_eq!(outline_width(4.), 1.);
    }

    #[test]
    fn escapes_ass_override_braces() {
        assert_eq!(escape_ass("a{b}c\\d"), "a\\{b\\}cd");
    }

    fn state_with_one_danmaku() -> DanmakuState {
        let mut state = DanmakuState {
            area: Some((640., 360.)),
            ..DanmakuState::default()
        };
        state.apply_metrics();
        let item = Danmaku {
            id: 7,
            time: 0.0,
            mode: DanmakuMode::Scroll,
            font_size: 25,
            color: 0x00ff_0000,
            text: "hello".into(),
        };
        let mut active = Vec::new();
        assert!(state.spawn(&item, 0.0, 640., &mut active));
        state.active = active;
        state
    }

    fn another_danmaku() -> Danmaku {
        Danmaku {
            id: 8,
            time: 0.0,
            mode: DanmakuMode::Scroll,
            font_size: 25,
            color: 0x00ff_ffff,
            text: "world".into(),
        }
    }

    #[test]
    fn pins_the_danmaku_under_the_pointer() {
        let mut state = state_with_one_danmaku();
        let (x, y) = state.active[0].position;
        let width = state.active[0].width;

        assert!(state.pin_at(x + width / 2., y + 2., 0.0, false));
        let pinned = state.pinned().expect("pinned");
        assert_eq!(pinned.id, 7);
        assert_eq!(pinned.text, "hello");
        assert!(state.ass(1.).is_empty());
        assert_eq!(state.lanes[state.active[0].lane], f64::INFINITY);
    }

    #[test]
    fn a_pinned_danmaku_holds_still_and_outlives_its_expiry() {
        let mut state = state_with_one_danmaku();
        let (x, y) = state.active[0].position;
        let lifetime = state.active[0].expires - state.active[0].start;
        assert!(state.pin_at(x + 2., y + 2., 0.0, false));

        for step in 1..=10 {
            state.update(f64::from(step) * 0.5, 640., 360.);
        }
        assert_eq!(state.active[0].position.0, x);
        assert!(state.pinned().is_some(), "a pinned danmaku must not expire");

        assert!(state.unpin(5.0));
        assert!(state.pinned().is_none());
        let item = &state.active[0];
        assert_eq!(item.start, 5.0);
        assert!((item.expires - item.start - lifetime).abs() < 0.001);
        assert!(!state.ass(1.).is_empty());
    }

    #[test]
    fn the_topmost_danmaku_wins_and_the_previous_pin_is_released() {
        let mut state = state_with_one_danmaku();
        let mut active = std::mem::take(&mut state.active);
        assert!(state.spawn(&another_danmaku(), 0.0, 640., &mut active));
        active[1].position = active[0].position;
        state.active = active;

        let (x, y) = state.active[0].position;
        assert!(state.pin_at(x + 2., y + 2., 0.0, false));
        assert_eq!(state.pinned().expect("pinned").id, 8);

        assert!(state.unpin(3.0));
        assert_eq!(state.active[0].start, 0.0);
        assert_eq!(state.active[1].start, 3.0);
    }

    #[test]
    fn the_like_affordance_belongs_to_the_pinned_area() {
        let mut state = state_with_one_danmaku();
        let (x, y) = state.active[0].position;
        let width = state.active[0].width;
        assert!(state.pin_at(x + 2., y + 2., 0.0, false));

        assert!(!state.pin_at(x + width + 40., y + 2., 0.5, false));
        assert!(state.pinned().is_some());

        assert!(state.pin_at(x + width + 200., y + 2., 1.0, false));
        assert!(state.pinned().is_none());
    }

    #[test]
    fn a_pointer_drag_keeps_the_pin_alive() {
        let mut state = state_with_one_danmaku();
        let (x, y) = state.active[0].position;
        assert!(state.pin_at(x + 2., y + 2., 0.0, false));

        // Dragging the selection past the text must not unfreeze it.
        assert!(!state.pin_at(x - 400., y + 2., 0.5, true));
        assert!(state.pinned().is_some());

        assert!(state.pin_at(x - 400., y + 2., 1.0, false));
        assert!(state.pinned().is_none());
    }

    #[test]
    fn unpinning_without_a_pin_does_nothing() {
        let mut state = state_with_one_danmaku();
        assert!(!state.unpin(1.0));
    }

    #[test]
    fn resizing_releases_the_pin() {
        let mut state = state_with_one_danmaku();
        let (x, y) = state.active[0].position;
        assert!(state.pin_at(x + 2., y + 2., 0.0, false));

        state.update(0.0, 1280., 720.);
        assert!(state.pinned().is_none());
    }
}
