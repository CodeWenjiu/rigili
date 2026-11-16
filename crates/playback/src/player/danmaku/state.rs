use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
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
    area: Option<Bounds<Pixels>>,
    scale: f32,
    lane_height: f32,
    lanes: Vec<f64>,
    active: Rc<RefCell<Vec<ActiveDanmaku>>>,
    settings: DanmakuSettings,
    font_family: SharedString,
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
    }

    pub(super) fn clear(&mut self) {
        self.items = Arc::new(Vec::new());
        self.cursor = 0;
        self.last_time = None;
        self.active.borrow_mut().clear();
        self.lanes.fill(0.);
    }

    pub(super) fn seek(&mut self, time: f64) {
        self.reset_at(time);
    }

    fn active_limit(&self) -> usize {
        active_limit(self.lanes.len())
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
                || active.len() >= self.active_limit()
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
        let mut font = window.text_style().font();
        if !self.font_family.is_empty() {
            font.family = self.font_family.clone();
        }
        let run = TextRun {
            len: text.len(),
            font,
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

fn is_discontinuity(last: f64, time: f64) -> bool {
    time < last - SEEK_BACK || time > last + SEEK_FORWARD
}

fn active_limit(lanes: usize) -> usize {
    (lanes * ACTIVE_PER_LANE).clamp(MIN_ACTIVE, MAX_ACTIVE)
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
    use super::DanmakuSettings;
    use super::{DanmakuState, MAX_ACTIVE, MIN_ACTIVE, active_limit, is_discontinuity};

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

    #[test]
    fn caps_active_danmaku_by_display_area() {
        assert_eq!(active_limit(1), MIN_ACTIVE);
        assert_eq!(active_limit(6), 24);
        assert_eq!(active_limit(21), 84);
        assert_eq!(active_limit(128), MAX_ACTIVE);
    }

    #[test]
    fn keeps_the_cursor_when_items_are_replaced() {
        use rigili_bilibili::{Danmaku, DanmakuMode};

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
        state.set_sorted(std::sync::Arc::new(vec![
            item(0.0),
            item(1.0),
            item(6.0),
            item(9.0),
        ]));

        assert_eq!(state.cursor, 2);
        assert_eq!(state.active_count(), 0);
    }
}
