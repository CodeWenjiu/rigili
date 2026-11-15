use gpui_kit::*;

pub trait Refresh: Render + 'static {
    fn refresh(&mut self, cx: &mut Context<Self>);

    fn can_refresh(&self) -> bool {
        true
    }
}
