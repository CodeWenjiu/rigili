use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(crate) fn profile_badge(
    label: impl Into<SharedString>,
    background: Hsla,
    foreground: Hsla,
) -> AnyElement {
    div()
        .flex_shrink_0()
        .px_2()
        .py_1()
        .rounded(px(4.))
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .child(label.into())
        .into_any_element()
}

pub(crate) fn level_badge(level: u32, background: Hsla, foreground: Hsla) -> AnyElement {
    div()
        .px_1()
        .rounded(px(3.))
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .child(format!("LV{level}"))
        .into_any_element()
}

pub(crate) fn profile_title(
    name: impl Into<SharedString>,
    level: Option<u32>,
    vip: Option<String>,
    official: Option<String>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let primary = theme.primary;

    h_flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_lg()
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(name.into()),
        )
        .when_some(level.filter(|level| *level > 0), |this, level| {
            this.child(profile_badge(
                format!("LV{level}"),
                primary.opacity(0.15),
                primary,
            ))
        })
        .when_some(vip, |this, label| {
            this.child(profile_badge(label, primary, theme.primary_foreground))
        })
        .when_some(official, |this, label| {
            this.child(div().max_w(px(220.)).truncate().child(profile_badge(
                label,
                theme.accent,
                theme.muted_foreground,
            )))
        })
        .into_any_element()
}

pub(crate) fn profile_stat(label: &'static str, value: String, cx: &App) -> AnyElement {
    v_flex()
        .items_center()
        .gap_1()
        .child(div().text_lg().child(value))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .into_any_element()
}
