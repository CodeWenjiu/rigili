rigili_macro::mod_prv!(
    grid,
    home,
    login,
    message,
    mine,
    platform,
    preview,
    search,
    shell,
    space
);

use crate::platform::assets::AppAssets;

fn main() {
    platform::gpu::prefer_vulkan();
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        cx.set_global(rigili_core::login::LoginState::default());
        platform::init(cx);
        rigili_core::backend::init(cx);
        preview::init(cx);

        if let Err(error) = shell::open_main(cx) {
            eprintln!("failed to open window: {error}");
        }
    });
}
