rigili_macro::mod_prv!(
    error, grid, home, login, mine, playback, platform, search, shell
);

use crate::platform::assets::AppAssets;

fn main() {
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        cx.set_global(login::LoginState::default());
        platform::init(cx);
        rigili_core::backend::init(cx);

        if let Err(error) = shell::open_main(cx) {
            eprintln!("failed to open window: {error}");
        }
    });
}
