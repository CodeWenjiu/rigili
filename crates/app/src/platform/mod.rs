use gpui_kit::App;

gpuili_macro::mod_prv!(http);
gpuili_macro::mod_pub!(crate, assets, runtime, theme);

pub(crate) fn init(cx: &mut App) {
    theme::apply(cx);
    runtime::init(cx);
    http::init(cx);
}
