pub(crate) fn log_level() -> Option<String> {
    rigili_monitor::enabled().then(|| {
        let level = std::env::var("RIGILI_MPV_LOG").unwrap_or_else(|_| "info".to_owned());
        if matches!(
            level.as_str(),
            "no" | "fatal" | "error" | "warn" | "info" | "v" | "debug" | "trace"
        ) {
            level
        } else {
            rigili_monitor::monitor!("mpv", "unknown RIGILI_MPV_LOG {level:?}, using info");
            "info".to_owned()
        }
    })
}
