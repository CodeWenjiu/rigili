use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

const WIDTH: Pixels = px(360.);

pub(super) fn open<C>(window: &mut Window, cx: &mut App, title: &'static str, content: Entity<C>)
where
    C: Render + 'static,
{
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.w(WIDTH).title(title).child(content.clone())
    });
}
