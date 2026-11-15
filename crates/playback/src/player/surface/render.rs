use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_player::{PlaybackPhase, VideoSize};

use super::super::controls::{Transport, speed_label};
use super::{Notice, PlayerView};

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
        let buffering = matches!(
            self.status.phase,
            PlaybackPhase::Buffering | PlaybackPhase::Loading
        );
        let notice = self.notice.map(|(notice, _)| match notice {
            Notice::Speed(speed) => speed_label(speed),
            Notice::Volume(volume) => format!("音量 {volume:.0}%"),
        });
        let paused = self.status.paused && !self.controls_visible;
        let native_active = self.native_requested && !self.native_failed && self.native.is_some();
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
                    .child(Spinner::new().large())
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
        if paused && !buffering && self.status.error.is_none() && notice.is_none() {
            overlays.push(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(80.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(hsla(0., 0., 0., 0.5))
                            .child(
                                Icon::new(IconName::Pause)
                                    .size(px(36.))
                                    .text_color(Hsla::white()),
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
                            .bg(hsla(0., 0., 0., 0.62))
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(Hsla::white())
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
                    .key_context(crate::shortcuts::VIDEO),
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
