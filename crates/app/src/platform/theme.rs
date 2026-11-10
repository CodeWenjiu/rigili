use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{App, Background, hsla, linear_color_stop, linear_gradient, px, rgb};

pub(crate) fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = "Source Han Sans SC".into();
        theme.font_size = px(16.);
        theme.radius = px(6.);
        theme.radius_lg = px(10.);
        theme.shadow = false;

        theme.background = rgb(0x18191c).into();
        theme.foreground = rgb(0xe3e5e7).into();
        theme.border = rgb(0x2b2c30).into();
        theme.muted = rgb(0x232427).into();
        theme.muted_foreground = rgb(0x9499a0).into();
        theme.primary = rgb(0xfb7299).into();
        theme.primary_foreground = rgb(0xffffff).into();
        theme.primary_hover = rgb(0xff85a2).into();
        theme.primary_active = rgb(0xe05c80).into();
        theme.button_primary = rgb(0xfb7299).into();
        theme.button_primary_foreground = rgb(0xffffff).into();
        theme.button_primary_hover = rgb(0xff85a2).into();
        theme.button_primary_active = rgb(0xe05c80).into();
        theme.accent = rgb(0x2a2b30).into();
        theme.accent_foreground = rgb(0xe3e5e7).into();
        theme.danger = rgb(0xf56c6c).into();
        theme.danger_foreground = rgb(0xffffff).into();
        theme.warning = rgb(0xf5a623).into();
        theme.warning_foreground = rgb(0xffffff).into();
        theme.success = rgb(0x67c23a).into();
        theme.success_foreground = rgb(0xffffff).into();
        theme.info = rgb(0x00aeec).into();
        theme.info_foreground = rgb(0xffffff).into();
        theme.link = rgb(0x00aeec).into();
        theme.link_hover = rgb(0x33beef).into();
        theme.link_active = rgb(0x0096c9).into();
        theme.sidebar = rgb(0x16171a).into();
        theme.sidebar_foreground = rgb(0xc9ccd0).into();
        theme.sidebar_accent = rgb(0x2a2b30).into();
        theme.sidebar_accent_foreground = rgb(0xfb7299).into();
        theme.sidebar_primary = rgb(0xfb7299).into();
        theme.sidebar_primary_foreground = rgb(0xffffff).into();
        theme.sidebar_border = rgb(0x24252a).into();
        theme.popover = rgb(0x202124).into();
        theme.popover_foreground = rgb(0xe3e5e7).into();
        theme.input = rgb(0x3a3c40).into();
        theme.ring = rgb(0xfb7299).into();
        theme.selection = rgb(0x4a2e39).into();
        theme.skeleton = rgb(0x242529).into();
        theme.scrollbar = rgb(0x18191c).into();
        theme.scrollbar_thumb = rgb(0x3a3c40).into();
        theme.scrollbar_thumb_hover = rgb(0x4a4c52).into();
        theme.title_bar = rgb(0x18191c).into();
        theme.title_bar_border = rgb(0x2b2c30).into();
    });
}

pub(crate) fn cover_scrim() -> Background {
    linear_gradient(
        180.,
        linear_color_stop(hsla(0., 0., 0., 0.), 0.),
        linear_color_stop(hsla(0., 0., 0., 0.75), 1.),
    )
}
