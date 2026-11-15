use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::shortcuts::{PLAYER, ToggleDanmaku};

use crate::settings::DanmakuSettings;

use super::surface::PlayerView;

pub(super) struct DanmakuBar<'a> {
    pub(super) settings: DanmakuSettings,
    pub(super) input: &'a Entity<InputState>,
    pub(super) sending: bool,
    pub(super) settings_open: bool,
    pub(super) can_send: bool,
    pub(super) player: WeakEntity<PlayerView>,
}

impl DanmakuBar<'_> {
    pub(super) fn render(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let weak = self.player.clone();
        let empty = self.input.read(cx).value().trim().is_empty();
        let settings_panel = self
            .settings_open
            .then(|| danmaku_settings_panel(self.settings, weak.clone(), muted));

        h_flex()
            .w_full()
            .max_w(px(480.))
            .items_center()
            .gap_3()
            .child(
                Button::new("danmaku-toggle")
                    .ghost()
                    .with_size(px(32.))
                    .icon(if self.settings.enabled {
                        IconName::MessageSquareText
                    } else {
                        IconName::MessageSquareOff
                    })
                    .text_color(if self.settings.enabled {
                        primary
                    } else {
                        muted
                    })
                    .accessibility_label(if self.settings.enabled {
                        "关闭弹幕"
                    } else {
                        "开启弹幕"
                    })
                    .tooltip_with_action(
                        if self.settings.enabled {
                            "关闭弹幕"
                        } else {
                            "开启弹幕"
                        },
                        &ToggleDanmaku,
                        Some(PLAYER),
                    )
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.toggle_danmaku(cx));
                        }
                    }),
            )
            .child(
                Popover::new("danmaku-settings-popover")
                    .anchor(Anchor::BottomLeft)
                    .offset(px(6.))
                    .open(self.settings_open)
                    .trigger(
                        Button::new("danmaku-settings")
                            .ghost()
                            .with_size(px(28.))
                            .icon(IconName::Settings)
                            .accessibility_label("弹幕设置")
                            .tooltip("弹幕设置"),
                    )
                    .on_open_change({
                        let weak = weak.clone();
                        move |open, _, cx| {
                            let _ = weak
                                .update(cx, |this, cx| this.set_danmaku_settings_open(*open, cx));
                        }
                    })
                    .when_some(settings_panel, |popover, panel| popover.child(panel)),
            )
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(self.input)
                        .small()
                        .cleanable(true)
                        .disabled(!self.can_send)
                        .suffix(
                            Button::new("danmaku-send")
                                .primary()
                                .xsmall()
                                .disabled(self.sending || empty || !self.can_send)
                                .label(if self.sending {
                                    "发送中…"
                                } else {
                                    "发送"
                                })
                                .on_click({
                                    let weak = self.player.clone();
                                    move |_, window, cx| {
                                        let _ = weak
                                            .update(cx, |this, cx| this.submit_danmaku(window, cx));
                                    }
                                }),
                        ),
                ),
            )
    }
}

const DANMAKU_OPACITIES: [(f32, &str); 4] =
    [(1.0, "100%"), (0.75, "75%"), (0.5, "50%"), (0.25, "25%")];
const DANMAKU_FONT_SCALES: [(f32, &str); 4] =
    [(0.8, "小"), (1.0, "标准"), (1.25, "大"), (1.5, "特大")];
const DANMAKU_SPEEDS: [(f32, &str); 3] = [(12.0, "慢"), (8.0, "标准"), (5.0, "快")];
const DANMAKU_AREAS: [(f32, &str); 4] = [(0.25, "1/4"), (0.5, "1/2"), (0.75, "3/4"), (1.0, "满屏")];

fn danmaku_settings_panel(
    settings: DanmakuSettings,
    weak: WeakEntity<PlayerView>,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .w(px(260.))
        .gap_3()
        .child(danmaku_settings_group(
            weak.clone(),
            "danmaku-opacity",
            "不透明度",
            &DANMAKU_OPACITIES,
            option_index(&DANMAKU_OPACITIES, settings.opacity),
            muted,
            with_opacity,
        ))
        .child(danmaku_settings_group(
            weak.clone(),
            "danmaku-font",
            "字号",
            &DANMAKU_FONT_SCALES,
            option_index(&DANMAKU_FONT_SCALES, settings.font_scale),
            muted,
            with_font_scale,
        ))
        .child(danmaku_settings_group(
            weak.clone(),
            "danmaku-speed",
            "滚动速度",
            &DANMAKU_SPEEDS,
            option_index(&DANMAKU_SPEEDS, settings.scroll_seconds as f32),
            muted,
            with_speed,
        ))
        .child(danmaku_settings_group(
            weak,
            "danmaku-area",
            "显示区域",
            &DANMAKU_AREAS,
            option_index(&DANMAKU_AREAS, settings.area_ratio),
            muted,
            with_area,
        ))
}

fn danmaku_settings_group(
    weak: WeakEntity<PlayerView>,
    id: &'static str,
    title: &'static str,
    options: &'static [(f32, &'static str)],
    selected: usize,
    muted: Hsla,
    apply: fn(DanmakuSettings, f32) -> DanmakuSettings,
) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xs().text_color(muted).child(title))
        .child(
            RadioGroup::horizontal(id)
                .selected_index(Some(selected))
                .children(options.iter().enumerate().map(|(index, (_, label))| {
                    Radio::new((id, index as u32))
                        .label(*label)
                        .accessibility_label(format!("{title} {label}"))
                }))
                .on_click(move |index, _, cx| {
                    let value = options[*index].0;
                    let _ = weak.update(cx, |this, cx| {
                        let settings = apply(this.danmaku_settings(cx), value);
                        this.apply_danmaku_settings(settings, cx);
                    });
                }),
        )
}

fn option_index(options: &[(f32, &'static str)], value: f32) -> usize {
    options
        .iter()
        .position(|(candidate, _)| (candidate - value).abs() < 1e-3)
        .unwrap_or(0)
}

fn with_opacity(mut settings: DanmakuSettings, value: f32) -> DanmakuSettings {
    settings.opacity = value;
    settings
}

fn with_font_scale(mut settings: DanmakuSettings, value: f32) -> DanmakuSettings {
    settings.font_scale = value;
    settings
}

fn with_speed(mut settings: DanmakuSettings, value: f32) -> DanmakuSettings {
    settings.scroll_seconds = f64::from(value);
    settings
}

fn with_area(mut settings: DanmakuSettings, value: f32) -> DanmakuSettings {
    settings.area_ratio = value;
    settings
}

#[cfg(test)]
mod tests {
    use super::{
        DANMAKU_FONT_SCALES, DanmakuSettings, option_index, with_font_scale, with_opacity,
    };

    #[test]
    fn maps_danmaku_settings_presets() {
        let settings = with_opacity(DanmakuSettings::default(), 0.5);
        assert_eq!(settings.opacity, 0.5);
        assert_eq!(option_index(&DANMAKU_FONT_SCALES, 1.25), 2);
        assert_eq!(option_index(&DANMAKU_FONT_SCALES, 9.0), 0);
        assert_eq!(with_font_scale(settings, 1.5).font_scale, 1.5);
    }
}
