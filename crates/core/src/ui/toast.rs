use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::component::notification::{Notification, NotificationList};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub fn init(cx: &mut App) {
    Root::register_plugin::<Toasts>(cx, Toasts::new);
}

pub fn push(window: &mut Window, cx: &mut App, note: impl Into<Notification>) {
    let Some(list) = list(window, cx) else {
        return;
    };
    list.update(cx, |list, cx| list.push(note, window, cx));
}

fn list(window: &Window, cx: &App) -> Option<Entity<NotificationList>> {
    let root = window.root::<Root>().flatten()?;
    let toasts = root.read(cx).plugin::<Toasts>()?;
    Some(toasts.read(cx).list.clone())
}

struct Toasts {
    list: Entity<NotificationList>,
}

impl Toasts {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let list = cx.new(|cx| NotificationList::new(window, cx));
        cx.observe(&list, |_, _, cx| cx.notify()).detach();
        Self { list }
    }
}

impl Render for Toasts {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        deferred(div().absolute().inset_0().child(self.list.clone()))
    }
}

impl RootPlugin for Toasts {}
