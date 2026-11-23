use std::f32::consts::TAU;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_player::{PlaybackPhase, VideoSize};

use super::super::controls::{Transport, speed_label};
use super::{
    HOLD_SPEED, Notice, OVERLAY_FOREGROUND, OVERLAY_SCRIM, OVERLAY_SCRIM_STRONG, PlayerView, SPEEDS,
};

const PAUSE_BADGE_MARGIN: Pixels = px(16.);
const PAUSE_BADGE_ABOVE_CONTROLS: Pixels = px(84.);
const SPEED_HOLD_TOP: Pixels = px(16.);
const SPEED_HOLD_PULSE: Duration = Duration::from_millis(900);
const SUBTITLE_MARGIN: f32 = 8.0;

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
        let native_active = self.native_requested && !self.native_failed && self.native.is_some();
        let has_picture = if native_active {
            self.native.as_ref().is_some_and(|native| native.visible)
        } else {
            self.frame.is_some()
        };
        let buffering = matches!(
            self.status.phase,
            PlaybackPhase::Buffering | PlaybackPhase::Loading
        ) || (!has_picture
            && self.status.error.is_none()
            && self.status.phase != PlaybackPhase::Idle);
        let notice = self.notice.map(|(notice, _)| match notice {
            Notice::Speed(speed) => speed_label(speed),
            Notice::Volume(volume) => format!("音量 {volume:.0}%"),
        });
        let paused = self.status.paused && self.status.error.is_none();
        let frame = (!native_active).then(|| self.frame.clone()).flatten();
        let video_bounds = self.video_bounds.clone();
        let controls_bounds = self.controls_bounds.clone();
        let player = self.player.clone();

        let mut overlays: Vec<AnyElement> = Vec::new();
        if let Some(pinned) = self.pinned_danmaku(cx) {
            overlays.push(pinned);
        }
        if let Some(subtitle) = self.subtitle_element(cx) {
            overlays.push(subtitle);
        }
        if self.controls_visible && self.subtitle_drag.is_none() {
            overlays.push(
                div()
                    .id("player-controls")
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_prepaint(move |bounds, _, _| controls_bounds.set(Some(bounds)))
                    .on_hover(cx.listener(|this, hovered, _, _| this.bar_hovered(*hovered)))
                    .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
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
                            subtitles: &self.subtitles,
                            selected_subtitle: self.subtitle_enabled.then_some(self.subtitle),
                            subtitle_open: self.subtitle_popover_open,
                            pages: &self.pages,
                            selected_cid: self.selected_cid,
                            pages_open: self.page_popover_open,
                            speeds: &SPEEDS,
                            speed_open: self.speed_popover_open,
                            preview: self.progress_preview(cx),
                            live: self.live,
                            danmaku: self.danmaku_settings(),
                            danmaku_input: &self.danmaku_input,
                            danmaku_sending: self.danmaku_sending,
                            danmaku_settings_open: self.danmaku_settings_open,
                            can_send_danmaku: self.danmaku_source.is_some(),
                            player: cx.entity().downgrade(),
                        }
                        .render(window, cx),
                    )
                    .into_any_element(),
            );
        } else {
            self.controls_bounds.set(None);
        }
        if buffering {
            overlays.push(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(64.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(OVERLAY_SCRIM)
                            .child(Spinner::new().large().color(OVERLAY_FOREGROUND)),
                    )
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
        if paused {
            let bottom = if self.controls_visible {
                PAUSE_BADGE_ABOVE_CONTROLS
            } else {
                PAUSE_BADGE_MARGIN
            };
            overlays.push(
                div()
                    .absolute()
                    .right(PAUSE_BADGE_MARGIN)
                    .bottom(bottom)
                    .child(
                        div()
                            .size(px(80.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(OVERLAY_SCRIM)
                            .child(
                                Icon::new(IconName::Pause)
                                    .size(px(36.))
                                    .text_color(OVERLAY_FOREGROUND),
                            ),
                    )
                    .into_any_element(),
            );
        }
        if self.speed_hold.is_some() {
            overlays.push(
                div()
                    .absolute()
                    .top(SPEED_HOLD_TOP)
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .bg(OVERLAY_SCRIM)
                            .text_color(OVERLAY_FOREGROUND)
                            .child(
                                Icon::new(IconName::FastForward)
                                    .size(px(18.))
                                    .text_color(OVERLAY_FOREGROUND)
                                    .with_animation(
                                        "speed-hold-pulse",
                                        Animation::new(SPEED_HOLD_PULSE).repeat().with_max_fps(30.),
                                        |icon, delta| {
                                            let phase = delta * TAU;
                                            icon.transform(Transformation::translate(point(
                                                px(phase.sin() * 3.),
                                                px(0.),
                                            )))
                                            .text_color(Hsla {
                                                a: 0.75 + 0.25 * phase.cos(),
                                                ..OVERLAY_FOREGROUND
                                            })
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(speed_label(HOLD_SPEED)),
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
                            .bg(OVERLAY_SCRIM_STRONG)
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(OVERLAY_FOREGROUND)
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
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "right"
                            && !event.keystroke.modifiers.modified()
                            && !event.is_held
                        {
                            this.begin_speed_hold(cx);
                        }
                    }))
                    .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                        if event.keystroke.key == "right" {
                            this.end_speed_hold(cx);
                        }
                    })),
            )
            .child(deferred(
                div()
                    .id("player-video-input")
                    .absolute()
                    .size_full()
                    .cursor(if self.subtitle_drag.is_some() {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::Arrow
                    })
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if this.subtitle_drag.is_some() {
                            if event.pressed_button == Some(MouseButton::Left) {
                                this.drag_subtitle(event.position, cx);
                            } else {
                                this.end_subtitle_drag(cx);
                            }
                            cx.stop_propagation();
                            return;
                        }
                        this.pointer_moved(cx);
                        this.hover_danmaku(
                            event.position,
                            event.pressed_button.is_some(),
                            window,
                            cx,
                        );
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_subtitle_drag(cx)),
                    )
                    .on_hover(cx.listener(|this, hovered, window, cx| {
                        if *hovered {
                            this.pointer_moved(cx);
                        } else {
                            this.pointer_left_video(cx);
                            this.leave_danmaku(window, cx);
                        }
                    }))
                    .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
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

impl PlayerView {
    fn subtitle_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let cue = self.active_subtitle_index()?;
        let file = self.subtitle_files.get(self.subtitle)?.as_ref()?;
        let line = file.body.get(cue)?;
        let text = line.content.trim();
        if text.is_empty() {
            return None;
        }

        let video = self.video_bounds.get()?;
        let width = f32::from(video.size.width);
        let height = f32::from(video.size.height);
        if width <= 0.0 || height <= 0.0 {
            return None;
        }

        let box_width = (width * 0.72)
            .clamp(180.0, 880.0)
            .min((width - 16.0).max(120.0));
        let box_height = self
            .subtitle_bounds
            .get()
            .map_or(0.0, |bounds| f32::from(bounds.size.height));
        let left = (self.subtitle_x * width - box_width / 2.0).clamp(
            SUBTITLE_MARGIN,
            (width - box_width - SUBTITLE_MARGIN).max(SUBTITLE_MARGIN),
        );
        let controls_top = if self.subtitle_drag.is_none() && self.controls_visible {
            self.controls_bounds
                .get()
                .map(|controls| f32::from(controls.top() - video.top()))
        } else {
            None
        };
        let top = subtitle_top(self.subtitle_y * height, box_height, height, controls_top);
        let font_size = (height * 0.034).clamp(14.0, 34.0);
        let radius = cx.theme().radius;
        let bounds_cell = self.subtitle_bounds.clone();

        Some(
            div()
                .id("player-subtitle")
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(box_width))
                .flex()
                .justify_center()
                .cursor(CursorStyle::OpenHand)
                .on_prepaint(move |bounds, _, _| bounds_cell.set(Some(bounds)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        this.begin_subtitle_drag(event.position, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    div()
                        .max_w_full()
                        .px_3()
                        .py_1()
                        .rounded(radius)
                        .bg(Hsla::black().opacity(0.62))
                        .text_center()
                        .text_color(OVERLAY_FOREGROUND)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(font_size))
                        .line_height(relative(1.35))
                        .child(text.to_owned()),
                )
                .into_any_element(),
        )
    }
}

fn subtitle_top(
    desired: f32,
    box_height: f32,
    video_height: f32,
    controls_top: Option<f32>,
) -> f32 {
    let max_top = (video_height - box_height - SUBTITLE_MARGIN).max(SUBTITLE_MARGIN);
    let top = desired.clamp(SUBTITLE_MARGIN, max_top);
    let Some(controls_top) = controls_top else {
        return top;
    };
    if top + box_height + SUBTITLE_MARGIN <= controls_top {
        return top;
    }
    (controls_top - box_height - SUBTITLE_MARGIN).clamp(SUBTITLE_MARGIN, max_top)
}

#[cfg(test)]
mod tests {
    use super::subtitle_top;

    #[test]
    fn keeps_a_subtitle_clear_of_the_controls() {
        assert_eq!(subtitle_top(500.0, 40.0, 720.0, Some(600.0)), 500.0);
    }

    #[test]
    fn lifts_a_subtitle_into_the_controls() {
        assert_eq!(subtitle_top(580.0, 40.0, 720.0, Some(600.0)), 552.0);
    }

    #[test]
    fn lifts_a_subtitle_dragged_onto_the_controls() {
        assert_eq!(subtitle_top(700.0, 40.0, 720.0, Some(600.0)), 552.0);
    }

    #[test]
    fn keeps_a_subtitle_above_the_controls_in_a_short_video() {
        assert_eq!(subtitle_top(50.0, 40.0, 60.0, Some(40.0)), 8.0);
    }

    #[test]
    fn places_a_subtitle_without_measuring_the_controls() {
        assert_eq!(subtitle_top(700.0, 40.0, 720.0, None), 672.0);
    }
}
