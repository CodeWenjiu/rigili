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
                            speeds: &SPEEDS,
                            speed_open: self.speed_popover_open,
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
