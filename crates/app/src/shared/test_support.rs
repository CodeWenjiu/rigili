use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, Entity, Render, TestAppContext, WindowBounds, WindowOptions,
    px, size,
};

pub(crate) fn open<V>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Context<V>) -> V,
) -> (AnyWindowHandle, Entity<V>)
where
    V: Render + 'static,
{
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Default::default(),
                size: size(px(800.), px(600.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(build)).expect("open test window")
    })
}

pub(crate) fn frames(cx: &mut TestAppContext, window: AnyWindowHandle, count: usize) {
    for _ in 0..count {
        cx.background_executor
            .advance_clock(Duration::from_millis(16));
        let _ = cx.update_window(window, |_, window, cx| {
            window.simulate_next_frame(cx);
        });
    }
}
