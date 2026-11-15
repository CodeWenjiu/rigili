rigili_macro::mod_prv!(app, pages, shared);

use crate::app::platform::assets::AppAssets;

fn main() {
    app::platform::gpu::prefer_vulkan();
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            rigili_playback::shortcuts::bind(cx);
            rigili_core::cursor::init(cx, platform.clone());
            cx.set_global(rigili_core::login::LoginState::default());
            app::platform::init(cx);
            rigili_core::backend::init(cx);
            shared::media::watch_later::init(cx);
            shared::media::preview::init(cx);

            if let Err(error) = app::shell::open_main(cx) {
                eprintln!("failed to open window: {error}");
            }
        });
}
