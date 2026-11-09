use gpui_kit::*;

gpuili_macro::mod_flat!(app);

fn main() {
    let mut args = std::env::args().skip(1);
    let initial_video = args.next();
    let initial_audio = args.next();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| MainView::new(window, cx, initial_video, initial_audio))
            })
            .expect("failed to open window");
        });
}
