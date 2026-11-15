use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::*;

const SMOOTH_MS: f32 = 45.;
const SMOOTH_STEP_MS: f32 = 16.;
const SMOOTH_STOP_PX: f32 = 0.5;
const COAST_MS: f32 = 130.;
const FRAME_MAX_MS: f32 = 50.;

const PRECISE_GAP_MS: f32 = 32.;
const VELOCITY_MAX: f32 = 7_000.;
const VELOCITY_STOP: f32 = 30.;

// 1.0 keeps the platform's delta; other values scale the scroll distance
// and, for touchpads, the momentum derived from it.
const WHEEL_SENSITIVITY: f32 = 1.0;
const PRECISE_SENSITIVITY: f32 = 1.5;

#[derive(Clone, Default)]
pub(crate) struct Motion(Rc<RefCell<State>>);

#[derive(Clone, Copy, Default)]
enum Kind {
    #[default]
    Idle,
    Smooth {
        remaining: f32,
    },
    Fling {
        velocity: f32,
    },
}

#[derive(Default)]
struct State {
    kind: Kind,
    last_event: Option<Instant>,
    last_tick: Option<Instant>,
    running: bool,
}

impl Motion {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn pending(&self) -> Pixels {
        match self.0.borrow().kind {
            Kind::Smooth { remaining } => px(remaining),
            Kind::Idle | Kind::Fling { .. } => px(0.),
        }
    }

    fn smooth(&self, delta: Pixels) {
        self.0.borrow_mut().kind = Kind::Smooth {
            remaining: delta.as_f32(),
        };
    }

    fn nudge(&self, now: Instant) -> Option<Pixels> {
        let mut state = self.0.borrow_mut();
        state.last_tick = Some(now);
        let Kind::Smooth { remaining } = state.kind else {
            return None;
        };
        if remaining.abs() < SMOOTH_STOP_PX {
            state.kind = Kind::Idle;
            return Some(px(remaining));
        }
        let decay = (-SMOOTH_STEP_MS / SMOOTH_MS).exp();
        state.kind = Kind::Smooth {
            remaining: remaining * decay,
        };
        Some(px(remaining * (1. - decay)))
    }

    fn fling(&self, delta: Pixels, now: Instant) -> bool {
        let mut state = self.0.borrow_mut();
        let elapsed = state
            .last_event
            .map(|last| now.saturating_duration_since(last).as_secs_f32() * 1000.);
        state.last_event = Some(now);

        let delta = delta.as_f32();
        if delta == 0. {
            return state.running;
        }

        let dt = elapsed.unwrap_or(PRECISE_GAP_MS).clamp(8., FRAME_MAX_MS);
        let instant = delta / (dt / 1000.);
        let velocity = match state.kind {
            Kind::Fling { velocity } if elapsed.is_some_and(|ms| ms <= FRAME_MAX_MS) => {
                velocity * 0.6 + instant * 0.4
            }
            Kind::Idle | Kind::Smooth { .. } | Kind::Fling { .. } => instant,
        }
        .clamp(-VELOCITY_MAX, VELOCITY_MAX);
        state.kind = Kind::Fling { velocity };
        velocity.abs() >= VELOCITY_STOP
    }

    fn start(&self, now: Instant) -> bool {
        let mut state = self.0.borrow_mut();
        if state.running {
            return false;
        }
        state.running = true;
        state.last_tick = Some(now);
        true
    }

    fn stop(&self) {
        let mut state = self.0.borrow_mut();
        state.running = false;
        state.kind = Kind::Idle;
    }

    fn running(&self) -> bool {
        self.0.borrow().running
    }

    fn tick(&self, now: Instant) -> Option<Pixels> {
        let mut state = self.0.borrow_mut();
        if !state.running {
            return None;
        }
        let Some(last_tick) = state.last_tick else {
            state.running = false;
            return None;
        };
        state.last_tick = Some(now);
        let dt = (now.saturating_duration_since(last_tick).as_secs_f32() * 1000.)
            .clamp(1., FRAME_MAX_MS);

        match state.kind {
            Kind::Idle => {
                state.running = false;
                None
            }
            Kind::Smooth { remaining } => {
                if remaining.abs() < SMOOTH_STOP_PX {
                    state.kind = Kind::Idle;
                    state.running = false;
                    return Some(px(remaining));
                }
                let decay = (-dt / SMOOTH_MS).exp();
                let step = remaining * (1. - decay);
                state.kind = Kind::Smooth {
                    remaining: remaining * decay,
                };
                Some(px(step))
            }
            Kind::Fling { velocity } => {
                let Some(last_event) = state.last_event else {
                    state.running = false;
                    return None;
                };
                if now.saturating_duration_since(last_event).as_secs_f32() * 1000. < PRECISE_GAP_MS
                {
                    return Some(px(0.));
                }
                if velocity.abs() < VELOCITY_STOP {
                    state.kind = Kind::Idle;
                    state.running = false;
                    return None;
                }
                let step = velocity * (dt / 1000.);
                state.kind = Kind::Fling {
                    velocity: velocity * (-dt / COAST_MS).exp(),
                };
                Some(px(step))
            }
        }
    }
}

pub(crate) struct ScrollArea {
    handle: ScrollHandle,
    motion: Motion,
}

impl ScrollArea {
    pub(crate) fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            motion: Motion::new(),
        }
    }

    pub(crate) fn listener<V: Render + 'static>(
        &self,
        cx: &Context<V>,
    ) -> impl Fn(&ScrollEvent, &mut Window, &mut App) + 'static {
        wheel_listener(
            self.motion.clone(),
            self.handle.clone(),
            cx.entity().downgrade(),
        )
    }
}

pub(crate) fn attach<V: Render + 'static>(
    div: Stateful<Div>,
    area: &ScrollArea,
    cx: &Context<V>,
) -> Stateful<Div> {
    div.track_scroll(&area.handle)
        .on_scroll(area.listener(cx))
        .on_mouse_down(MouseButton::Left, cancel_listener(area.motion.clone()))
}

pub(crate) fn wheel_listener<V: Render + 'static>(
    motion: Motion,
    handle: ScrollHandle,
    weak: WeakEntity<V>,
) -> impl Fn(&ScrollEvent, &mut Window, &mut App) + 'static {
    move |event, window, cx| {
        if event.source != ScrollSource::Pointer {
            return;
        }
        let _ = weak.update(cx, |_, cx| wheel(&motion, &handle, event, window, cx));
    }
}

pub(crate) fn cancel_listener(
    motion: Motion,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    move |_, _, _| motion.stop()
}

fn is_precise(delta: ScrollDelta) -> bool {
    // Wayland reports touchpads as pixel deltas and X11 as fractional lines;
    // integer line deltas are discrete mouse-wheel notches.
    match delta {
        ScrollDelta::Pixels(_) => true,
        ScrollDelta::Lines(lines) => {
            (lines.y - lines.y.round()).abs() > 0.01 || (lines.x - lines.x.round()).abs() > 0.01
        }
    }
}

fn wheel<V: Render + 'static>(
    motion: &Motion,
    handle: &ScrollHandle,
    event: &ScrollEvent,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let raw = event.delta.pixel_delta(window.line_height()).y;
    if raw == px(0.) {
        return;
    }
    let precise = is_precise(event.delta);
    let sensitivity = if precise {
        PRECISE_SENSITIVITY
    } else {
        WHEEL_SENSITIVITY
    };
    let scaled = px(raw.as_f32() * sensitivity);
    let now = cx.background_executor().now();

    if cx.reduce_motion() {
        if apply_extra(handle, scaled - raw) {
            cx.notify();
        }
        return;
    }

    if precise {
        if apply_extra(handle, scaled - raw) {
            cx.notify();
        }
        if !motion.fling(scaled, now) || !motion.start(now) {
            return;
        }
        schedule::<V>(motion.clone(), handle.clone(), window, cx);
        return;
    }

    let max = handle.max_offset().y;
    let post = handle.offset().y;
    if (post == px(0.) && raw > px(0.)) || (post == -max && raw < px(0.)) {
        return;
    }

    let pre = (post - raw).clamp(-max, px(0.));
    let pending = motion.pending();
    let pending = if pending != px(0.) && pending.as_f32().signum() != scaled.as_f32().signum() {
        px(0.)
    } else {
        pending
    };
    let target = (pre + pending + scaled).clamp(-max, px(0.));
    let remaining = target - pre;

    let offset = handle.offset();
    handle.set_offset(point(offset.x, pre));
    if remaining == px(0.) {
        motion.stop();
        return;
    }

    motion.smooth(remaining);
    if let Some(first) = motion.nudge(now) {
        let offset = handle.offset();
        let max = handle.max_offset();
        let y = (offset.y + first).clamp(-max.y, px(0.));
        if y != offset.y {
            handle.set_offset(point(offset.x, y));
            cx.notify();
        }
    }
    if motion.start(now) {
        schedule::<V>(motion.clone(), handle.clone(), window, cx);
    }
}

fn apply_extra(handle: &ScrollHandle, extra: Pixels) -> bool {
    if extra == px(0.) {
        return false;
    }
    let offset = handle.offset();
    let max = handle.max_offset();
    let y = (offset.y + extra).clamp(-max.y, px(0.));
    if y == offset.y {
        return false;
    }
    handle.set_offset(point(offset.x, y));
    true
}

fn schedule<V: Render + 'static>(
    motion: Motion,
    handle: ScrollHandle,
    window: &Window,
    cx: &Context<V>,
) {
    let weak = cx.entity().downgrade();
    window.on_next_frame(move |window, cx| tick::<V>(&motion, &handle, &weak, window, cx));
}

fn tick<V: Render + 'static>(
    motion: &Motion,
    handle: &ScrollHandle,
    weak: &WeakEntity<V>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(step) = motion.tick(cx.background_executor().now()) else {
        return;
    };
    if step == px(0.) {
        reschedule::<V>(motion, handle, weak, window);
        return;
    }

    let offset = handle.offset();
    let max = handle.max_offset();
    let y = (offset.y + step).clamp(-max.y, px(0.));
    if y == offset.y {
        motion.stop();
        return;
    }

    handle.set_offset(point(offset.x, y));
    if weak.update(cx, |_, cx| cx.notify()).is_err() {
        motion.stop();
        return;
    }

    if !motion.running() {
        return;
    }
    reschedule::<V>(motion, handle, weak, window);
}

fn reschedule<V: Render + 'static>(
    motion: &Motion,
    handle: &ScrollHandle,
    weak: &WeakEntity<V>,
    window: &Window,
) {
    let motion = motion.clone();
    let handle = handle.clone();
    let weak = weak.clone();
    window.on_next_frame(move |window, cx| tick::<V>(&motion, &handle, &weak, window, cx));
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        Context, IntoElement, Render, ScrollDelta, TestAppContext, TestSupportExt as _, Window,
        div, point, px,
    };

    use super::{Motion, ScrollArea, attach};
    use crate::shared::test_support::{frames, open};

    fn fling(motion: &Motion, base: Instant, at: u64, delta: f32) -> bool {
        motion.fling(px(delta), base + Duration::from_millis(at))
    }

    #[test]
    fn precise_input_waits_for_the_gesture_then_coasts() {
        let motion = Motion::new();
        let base = Instant::now();
        fling(&motion, base, 0, -8.);
        assert!(fling(&motion, base, 8, -8.));
        assert!(fling(&motion, base, 16, -8.));

        let started = base + Duration::from_millis(16);
        assert!(motion.start(started));

        let waiting = motion
            .tick(started + Duration::from_millis(8))
            .expect("the gesture is still active");
        assert_eq!(waiting, px(0.));

        let coasting = motion
            .tick(started + Duration::from_millis(40))
            .expect("the coast should start after the pause");
        assert!(coasting < px(0.));
    }

    #[test]
    fn a_stopped_motion_does_not_step() {
        let motion = Motion::new();
        assert!(motion.tick(Instant::now()).is_none());
    }

    struct Fixture {
        area: ScrollArea,
    }

    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            attach(
                div()
                    .id("scroller")
                    .size_full()
                    .overflow_y_scroll()
                    .child(div().h(px(4000.))),
                &self.area,
                cx,
            )
            .test_support()
        }
    }

    #[gpui_kit::test]
    fn wheel_notches_scroll_smoothly_to_the_target(cx: &mut TestAppContext) {
        let (window, view) = open(cx, |_| Fixture {
            area: ScrollArea::new(),
        });
        let handle = cx.update(|cx| view.read(cx).area.handle.clone());
        let mut line = px(0.);

        let _ = cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            line = window.line_height();
            window.scroll("scroller", ScrollDelta::Lines(point(0., -3.)), cx);
            window.scroll("scroller", ScrollDelta::Lines(point(0., -3.)), cx);
        });

        let target = px(-6. * line.as_f32());
        let immediate = handle.offset().y;
        assert!(
            immediate > target * 0.5,
            "the notch should not jump to the target: {immediate:?}"
        );

        frames(cx, window, 60);

        let settled = handle.offset().y;
        assert!(
            (settled - target).abs() <= px(1.),
            "the wheel should settle at the default target: {settled:?} vs {target:?}"
        );

        frames(cx, window, 30);
        assert_eq!(
            handle.offset().y,
            settled,
            "mouse-wheel notches must not coast"
        );
    }

    #[gpui_kit::test]
    fn precise_scrolling_coasts_after_the_gesture(cx: &mut TestAppContext) {
        let (window, view) = open(cx, |_| Fixture {
            area: ScrollArea::new(),
        });
        let handle = cx.update(|cx| view.read(cx).area.handle.clone());

        for _ in 0..4 {
            let _ = cx.update_window(window, |_, window, cx| {
                window.scroll("scroller", ScrollDelta::Pixels(point(px(0.), px(-6.))), cx);
            });
            cx.background_executor
                .advance_clock(Duration::from_millis(8));
        }
        let after_gesture = handle.offset().y;
        assert!(after_gesture < px(0.), "the gesture should scroll");

        frames(cx, window, 30);

        let after_coast = handle.offset().y;
        assert!(
            after_coast < after_gesture,
            "momentum should continue after the gesture: {after_gesture:?} -> {after_coast:?}"
        );
    }
}
