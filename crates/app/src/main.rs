rigili_macro::mod_prv!(app, pages, shared);

use crate::app::platform::assets::AppAssets;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn tune_allocator() {
    // mimalloc defers purging freed pages by `purge_delay` milliseconds. Threads
    // that touch multi-megabyte buffers keep one such page each while idle, so
    // purge eagerly to keep the resident set close to live data.
    const MI_OPTION_PURGE_DELAY: libmimalloc_sys::mi_option_t = 15;
    // SAFETY: this only updates mimalloc's global option table and runs before
    // any worker thread exists.
    unsafe { libmimalloc_sys::mi_option_set(MI_OPTION_PURGE_DELAY, 0) };

    // mimalloc requests 2MiB transparent huge pages, so fragmented heaps round
    // every touched region up to 2MiB and never hand the slack back.
    // `libc` only exposes this prctl command on Android and Fuchsia.
    #[cfg(target_os = "linux")]
    const PR_SET_THP_DISABLE: libc::c_int = 41;
    #[cfg(target_os = "linux")]
    // SAFETY: `PR_SET_THP_DISABLE` ignores the remaining arguments.
    unsafe {
        libc::prctl(PR_SET_THP_DISABLE, 1, 0, 0, 0);
    }
}

fn main() {
    tune_allocator();
    app::platform::gpu::prefer_vulkan();
    install_diagnostics();
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            rigili_core::init_shortcut_help(cx);
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
