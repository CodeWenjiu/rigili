use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_player::{PlaybackPhase, PlayerStatus};

use super::surface::PlayerView;
use rigili_core::format_clock;

pub(super) struct Transport<'a> {
    pub(super) status: &'a PlayerStatus,
    pub(super) seeking: Option<f64>,
    pub(super) progress: &'a Entity<SliderState>,
    pub(super) volume: &'a Entity<SliderState>,
    pub(super) live: bool,
    pub(super) player: WeakEntity<PlayerView>,
}

impl Transport<'_> {
    pub(super) fn render(&self, window: &Window, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let primary = cx.theme().primary;
        let weak = self.player.clone();
        let display_time = self.seeking.unwrap_or(self.status.time_pos);
        let duration = self.status.duration.max(0.0);
        let paused = self.status.paused;
        let speed = self.status.speed;
        let fullscreen = window.is_fullscreen();

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(border)
            .child(
                Button::new("toggle-pause")
                    .ghost()
                    .icon(if paused {
                        IconName::Play
                    } else {
                        IconName::Pause
                    })
                    .accessibility_label(if paused { "播放" } else { "暂停" })
                    .tooltip(if paused { "播放" } else { "暂停" })
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.toggle_pause(cx));
                        }
                    }),
            )
            .when(!self.live, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format_clock(display_time)),
                )
                .child(
                    Slider::new(self.progress)
                        .flex_1()
                        .accessibility_label("播放进度"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format_clock(duration)),
                )
            })
            .when(self.live, |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(primary)
                        .child(div().size_2().rounded_full().bg(primary))
                        .child("直播中"),
                )
                .child(div().flex_1())
            })
            .child(div().text_sm().text_color(muted).child("音量"))
            .child(
                Slider::new(self.volume)
                    .w(px(110.0))
                    .accessibility_label("音量"),
            )
            .child(
                Button::new("speed")
                    .ghost()
                    .label(speed_label(speed))
                    .tooltip("切换倍速")
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.cycle_speed(cx));
                        }
                    }),
            )
            .child(
                Button::new("fullscreen")
                    .ghost()
                    .icon(if fullscreen {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .accessibility_label(if fullscreen { "退出全屏" } else { "全屏" })
                    .tooltip(if fullscreen { "退出全屏" } else { "全屏" })
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.toggle_fullscreen(window, cx);
                            });
                        }
                    }),
            )
    }
}

pub(super) fn status_bar(status: &PlayerStatus, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let danger = cx.theme().danger;
    let resolution = status
        .video_size
        .map(|size| format!("{}×{}", size.width, size.height))
        .unwrap_or_else(|| "—".into());
    let decoder = decoder_label(&status.hwdec);
    let state = phase_label(status);

    let mut details = vec![resolution, decoder];
    if let Some(codec) = &status.audio_codec {
        details.push(format!("音频 {codec}"));
    }
    details.push(state.to_string());

    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap_3()
        .px_3()
        .py_1()
        .text_xs()
        .text_color(muted)
        .child(details.join(" · "))
        .when(status.error.is_some(), |this| {
            this.child(div().text_color(danger).child("播放错误"))
        })
}

fn phase_label(status: &PlayerStatus) -> &'static str {
    match status.phase {
        PlaybackPhase::Loading => "加载中",
        PlaybackPhase::Buffering => "缓冲中",
        PlaybackPhase::Ended => "播放结束",
        PlaybackPhase::Idle => "未播放",
        PlaybackPhase::Playing if status.paused => "已暂停",
        PlaybackPhase::Playing => "播放中",
    }
}

fn speed_label(speed: f64) -> String {
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}x")
}

fn decoder_label(hwdec: &Option<String>) -> String {
    match hwdec.as_deref() {
        Some("no") => "软件解码".into(),
        Some(value) if !value.is_empty() => format!("硬件解码 {value}"),
        _ => "解码未知".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{decoder_label, phase_label, speed_label};
    use rigili_player::{PlaybackPhase, PlayerStatus};

    #[test]
    fn labels_every_phase() {
        let mut status = PlayerStatus::default();
        for (phase, paused, label) in [
            (PlaybackPhase::Idle, false, "未播放"),
            (PlaybackPhase::Loading, false, "加载中"),
            (PlaybackPhase::Buffering, false, "缓冲中"),
            (PlaybackPhase::Ended, false, "播放结束"),
            (PlaybackPhase::Playing, false, "播放中"),
            (PlaybackPhase::Playing, true, "已暂停"),
        ] {
            status.phase = phase;
            status.paused = paused;
            assert_eq!(phase_label(&status), label);
        }
    }

    #[test]
    fn labels_speeds_and_decoders() {
        assert_eq!(speed_label(1.0), "1x");
        assert_eq!(speed_label(1.25), "1.25x");
        assert_eq!(decoder_label(&Some("vaapi".into())), "硬件解码 vaapi");
        assert_eq!(decoder_label(&Some("no".into())), "软件解码");
        assert_eq!(decoder_label(&None), "解码未知");
    }
}
