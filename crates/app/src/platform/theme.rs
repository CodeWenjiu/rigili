use std::rc::Rc;

use gpui_kit::component::theme::{Theme, ThemeConfig};
use gpui_kit::{App, Background, hsla, linear_color_stop, linear_gradient};

const THEME: &str = r##"{
  "name": "gpuili",
  "mode": "dark",
  "font.family": "Source Han Sans SC",
  "font.size": 16,
  "radius": 6,
  "radius.lg": 10,
  "shadow": false,
  "colors": {
    "background": "#18191c",
    "foreground": "#e3e5e7",
    "border": "#2b2c30",
    "muted.background": "#232427",
    "muted.foreground": "#9499a0",
    "primary.background": "#fb7299",
    "primary.foreground": "#ffffff",
    "primary.hover.background": "#ff85a2",
    "primary.active.background": "#e05c80",
    "accent.background": "#2a2b30",
    "accent.foreground": "#e3e5e7",
    "danger.background": "#f56c6c",
    "danger.foreground": "#ffffff",
    "warning.background": "#f5a623",
    "warning.foreground": "#ffffff",
    "success.background": "#67c23a",
    "success.foreground": "#ffffff",
    "info.background": "#00aeec",
    "info.foreground": "#ffffff",
    "link": "#00aeec",
    "link.hover": "#33beef",
    "link.active": "#0096c9",
    "sidebar.background": "#16171a",
    "sidebar.foreground": "#c9ccd0",
    "sidebar.accent.background": "#2a2b30",
    "sidebar.accent.foreground": "#fb7299",
    "sidebar.primary.background": "#fb7299",
    "sidebar.primary.foreground": "#ffffff",
    "sidebar.border": "#24252a",
    "popover.background": "#202124",
    "popover.foreground": "#e3e5e7",
    "input.border": "#3a3c40",
    "ring": "#fb7299",
    "selection.background": "#4a2e39",
    "skeleton.background": "#242529",
    "scrollbar.background": "#18191c",
    "scrollbar.thumb.background": "#3a3c40",
    "scrollbar.thumb.hover.background": "#4a4c52",
    "title_bar.background": "#18191c",
    "title_bar.border": "#2b2c30"
  }
}"##;

pub(crate) fn apply(cx: &mut App) {
    let config = serde_json::from_str::<ThemeConfig>(THEME).expect("built-in theme must be valid");
    Theme::update(cx, |theme| theme.apply_config(&Rc::new(config)));
}

pub(crate) fn cover_scrim() -> Background {
    linear_gradient(
        180.,
        linear_color_stop(hsla(0., 0., 0., 0.), 0.),
        linear_color_stop(hsla(0., 0., 0., 0.75), 1.),
    )
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::theme::ThemeConfig;

    #[test]
    fn built_in_theme_parses() {
        let config = serde_json::from_str::<ThemeConfig>(super::THEME).expect("theme");
        assert_eq!(config.name.as_ref(), "gpuili");
        assert!(config.mode.is_dark());
    }
}
