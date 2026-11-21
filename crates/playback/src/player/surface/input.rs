use std::time::Instant;

use gpui_kit::component::slider::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::normalize_url;
use rigili_core::format_clock;

use super::{
    CONTROLS_IDLE, OVERLAY_FOREGROUND, OVERLAY_SCRIM_STRONG, PRECISE_VOLUME_STEP, PREVIEW_WIDTH,
    PlayerView, WHEEL_VOLUME_STEP,
};

impl PlayerView {
    pub(super) fn on_scroll(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(crate) fn preview_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
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

    pub(crate) fn preview_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.preview_hovered != hovered {
            self.preview_hovered = hovered;
            cx.notify();
        }
        if !hovered && self.seeking.is_none() && self.preview_time.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn progress_preview(&self, cx: &App) -> Option<AnyElement> {
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
                                .bg(OVERLAY_SCRIM_STRONG)
                                .text_xs()
                                .text_color(OVERLAY_FOREGROUND)
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

    pub(super) fn pointer_moved(&mut self, cx: &mut Context<Self>) {
        self.pointer_in_video = true;
        self.last_activity = Instant::now();
        if !self.controls_visible {
            self.controls_visible = true;
            cx.notify();
        }
    }

    pub(super) fn pointer_left_video(&mut self, cx: &mut Context<Self>) {
        self.pointer_in_video = false;
        self.pointer_in_bar = false;
        if self.controls_visible && !self.bar_interacting() {
            self.controls_visible = false;
            self.preview_time = None;
            cx.notify();
        }
    }

    pub(super) fn bar_hovered(&mut self, hovered: bool) {
        self.pointer_in_bar = hovered;
        if hovered {
            self.last_activity = Instant::now();
        }
    }

    pub(crate) fn set_volume_popup_open(&mut self, open: bool) {
        self.volume_popup_open = open;
        if open {
            self.last_activity = Instant::now();
        }
    }

    pub(super) fn bar_interacting(&self) -> bool {
        self.pointer_in_bar
            || self.seeking.is_some()
            || self.volume_dragging
            || self.volume_popup_open
            || self.quality_popover_open
            || self.speed_popover_open
            || self.page_popover_open
            || self.danmaku_settings_open
            || self.danmaku_sending
            || self.danmaku_input_focused
    }

    pub(super) fn controls_idle(&self) -> bool {
        self.controls_visible
            && !self.bar_interacting()
            && self.preview_time.is_none()
            && self.last_activity.elapsed() >= CONTROLS_IDLE
    }

    pub(super) fn on_progress(
        &mut self,
        event: &SliderEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                self.play_anchor = Some((*value as f64, Instant::now()));
                self.seek_danmaku(*value as f64, cx);
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

    pub(super) fn on_volume(
        &mut self,
        event: &SliderEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn set_volume(&mut self, volume: f64, window: &mut Window, cx: &mut Context<Self>) {
        self.status.volume = volume;
        self.player.set_volume(volume);
        self.volume
            .update(cx, |slider, cx| slider.set_value(volume as f32, window, cx));
        cx.notify();
    }
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

#[cfg(test)]
mod tests {
    use super::scroll_volume_step;
    use gpui_kit::{ScrollDelta, point, px};

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
}
