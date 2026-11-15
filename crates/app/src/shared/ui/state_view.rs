use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Layout {
    #[default]
    Top,
    Centered,
}

pub(crate) fn error_state(
    layout: Layout,
    message: impl Into<SharedString>,
    retry_id: impl Into<ElementId>,
    danger: Hsla,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .gap_3()
        .when(layout == Layout::Centered, |this| {
            this.items_center().justify_center()
        })
        .child(div().text_sm().text_color(danger).child(message.into()))
        .child(
            Button::new(retry_id)
                .primary()
                .label("重试")
                .on_click(on_retry),
        )
        .into_any_element()
}

pub(crate) fn empty_state(
    layout: Layout,
    muted: Hsla,
    message: impl Into<SharedString>,
) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .when(layout == Layout::Centered, |this| {
            this.items_center().justify_center()
        })
        .child(div().text_sm().text_color(muted).child(message.into()))
        .into_any_element()
}

pub(crate) fn centered_spinner() -> AnyElement {
    centered_spinner_with(Spinner::new())
}

pub(crate) fn centered_small_spinner() -> AnyElement {
    centered_spinner_with(Spinner::new().small())
}

fn centered_spinner_with(spinner: Spinner) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .child(spinner)
        .into_any_element()
}
