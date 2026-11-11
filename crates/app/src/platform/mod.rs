use std::borrow::Cow;

use gpui_kit::App;

rigili_macro::mod_prv!(http);
rigili_macro::mod_pub!(crate, assets, theme);

pub(crate) fn init(cx: &mut App) {
    load_ui_font(cx);
    theme::apply(cx);
    rigili_core::runtime::init(cx);
    http::init(cx);
}

fn load_ui_font(cx: &App) {
    let font = Cow::Borrowed(&include_bytes!("../../assets/fonts/LXGWNeoXiHei.ttf")[..]);
    if let Err(error) = cx.text_system().add_fonts(vec![font]) {
        eprintln!("failed to load the UI font: {error}");
    }
}
