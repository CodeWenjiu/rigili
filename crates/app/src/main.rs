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

#[cfg(feature = "video-monitor")]
fn monitor_log() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{Layer as _, filter::LevelFilter, layer::SubscriberExt as _};

    let dir = rigili_bilibili::data_dir().unwrap_or_else(std::env::temp_dir);
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("cannot create {}: {error}", dir.display());
        return None;
    }
    let path = dir.join("monitor.log");
    if let Err(error) = create_log_file(&path) {
        eprintln!("cannot create {}: {error}", path.display());
        return None;
    }

    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::never(&dir, "monitor.log"));
    // Keep the crates we depend on quiet: their INFO chatter is not what a
    // playback session needs, but their warnings can still explain a stall.
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(LevelFilter::WARN)
        .with_target("rigili", LevelFilter::INFO);
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_thread_names(true)
            .with_filter(filter),
    );
    if let Err(error) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("cannot install the monitor log: {error}");
        return None;
    }
    eprintln!("monitor log: {}", path.display());
    Some(guard)
}

#[cfg(not(feature = "video-monitor"))]
fn monitor_log() -> Option<()> {
    None
}

#[cfg(all(unix, feature = "video-monitor"))]
fn create_log_file(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(all(not(unix), feature = "video-monitor"))]
fn create_log_file(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(drop)
}

#[cfg(feature = "video-monitor")]
fn watch_memory() {
    // Long sessions grow on both sides of the FFI boundary, and only mimalloc
    // can report its own share: whatever RSS exceeds it belongs to mpv, ffmpeg
    // and the graphics stack.
    std::thread::Builder::new()
        .name("mem-watch".into())
        .spawn(|| {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(30));
                let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
                let mut commit = 0usize;
                let mut peak_commit = 0usize;
                // SAFETY: `mi_process_info` only writes the counters it is
                // handed; the statistics arguments are optional.
                unsafe {
                    libmimalloc_sys::mi_process_info(
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut commit,
                        &mut peak_commit,
                        std::ptr::null_mut(),
                    );
                }
                rigili_monitor::monitor!(
                    "mem",
                    "rss={}MB swap={}MB threads={} mimalloc={}MB peak={}MB",
                    status_kb(&status, "VmRSS:") / 1024,
                    status_kb(&status, "VmSwap:") / 1024,
                    status_kb(&status, "Threads:"),
                    commit / (1024 * 1024),
                    peak_commit / (1024 * 1024)
                );
            }
        })
        .ok();
}

#[cfg(not(feature = "video-monitor"))]
fn watch_memory() {}

#[cfg(feature = "video-monitor")]
fn status_kb(status: &str, name: &str) -> u64 {
    status
        .lines()
        .find_map(|line| line.strip_prefix(name))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn main() {
    tune_allocator();
    let _monitor_log = monitor_log();
    watch_memory();
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
            rigili_core::emotes::init(cx);
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
