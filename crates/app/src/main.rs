rigili_macro::mod_prv!(
    filter,
    grid,
    home,
    list,
    login,
    message,
    mine,
    paging,
    platform,
    preview,
    profile,
    scroll,
    search,
    season,
    shell,
    space,
    state_view,
    tabs,
    watch_later
);

use crate::platform::assets::AppAssets;

fn main() {
    platform::gpu::prefer_vulkan();
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            rigili_core::cursor::init(cx, platform.clone());
            cx.set_global(rigili_core::login::LoginState::default());
            platform::init(cx);
            rigili_core::backend::init(cx);
            watch_later::init(cx);
            preview::init(cx);

            if let Err(error) = shell::open_main(cx) {
                eprintln!("failed to open window: {error}");
            }
        });
}
