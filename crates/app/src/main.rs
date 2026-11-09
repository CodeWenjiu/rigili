use gpui_kit::*;

gpuili_macro::mod_flat!(app);

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| MainView))
                .expect("failed to open window");
        });
}
