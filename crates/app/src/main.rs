gpuili_macro::mod_prv!(backend, format, home, login, playback, platform, shell);

use crate::platform::assets::AppAssets;

fn main() {
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        platform::init(cx);

        if let Err(error) = shell::open_main(cx) {
            eprintln!("failed to open window: {error}");
        }
    });
}
