use gpui_kit::*;

gpuili_macro::mod_flat!(
    app,
    app_assets,
    backend,
    detail,
    format,
    home,
    http,
    player_view,
    runtime
);
gpuili_macro::mod_prv!(theme);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    Detail,
    Player,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let initial_video = args.next();
    let initial_audio = args.next();

    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        theme::apply(cx);
        runtime::init(cx);
        http::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| MainView::new(window, cx, initial_video, initial_audio))
        })
        .expect("failed to open window");
    });
}
