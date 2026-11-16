rigili_macro::mod_prv!(app, pages, shared);

use crate::app::platform::assets::AppAssets;

fn main() {
    app::platform::gpu::prefer_vulkan();
    install_diagnostics();
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_cursor_hide_mode(gpui_kit::CursorHideMode::Never);
            app::shortcuts::bind(cx);
            rigili_playback::shortcuts::bind(cx);
            rigili_playback::settings::init(cx);
            app::preferences::init(cx);
            rigili_core::cursor::init(cx, platform.clone());
            cx.set_global(rigili_core::login::LoginState::default());
            app::platform::init(cx);
            rigili_core::client::init(cx);
            shared::media::watch_later::init(cx);
            shared::media::preview::init(cx);

            if let Err(error) = app::shell::open_main(cx) {
                eprintln!("failed to open window: {error}");
            }
        });
}

#[cfg(feature = "video-monitor")]
fn install_diagnostics() {
    struct StderrLogger;

    impl log::Log for StderrLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: StderrLogger = StderrLogger;
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Warn));
}

#[cfg(not(feature = "video-monitor"))]
fn install_diagnostics() {}
