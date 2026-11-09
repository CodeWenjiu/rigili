use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

pub struct MainView;

impl Render for MainView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child("gpuili")
            .child(Button::new("hello").primary().label("Hello"))
    }
}
