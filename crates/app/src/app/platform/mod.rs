use std::borrow::Cow;

use gpui_kit::App;

rigili_macro::mod_prv!(http);
rigili_macro::mod_pub!(crate, assets, gpu, theme);

pub(crate) fn init(cx: &mut App) {
    load_ui_font(cx);
    theme::apply(cx);
    rigili_core::runtime::init(cx);
    http::init(cx);
    rigili_core::image_cache::init(cx);
}

fn load_ui_font(cx: &App) {
    let font = Cow::Borrowed(rigili_core::fonts::bytes());
    if let Err(error) = cx.text_system().add_fonts(vec![font]) {
        eprintln!("failed to load the UI font: {error}");
    }
}
