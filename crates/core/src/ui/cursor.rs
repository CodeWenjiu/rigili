use std::rc::Rc;

use gpui_kit::{App, Global, Platform};

struct CursorPlatform(Rc<dyn Platform>);

impl Global for CursorPlatform {}

pub fn init(cx: &mut App, platform: Rc<dyn Platform>) {
    cx.set_global(CursorPlatform(platform));
}

pub fn hide_until_mouse_moves(cx: &App) {
    if let Some(cursor) = cx.try_global::<CursorPlatform>() {
        cursor.0.hide_cursor_until_mouse_moves();
    }
}
