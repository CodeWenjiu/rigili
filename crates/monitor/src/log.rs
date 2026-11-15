#[macro_export]
macro_rules! monitor {
    ($target:expr, $($arg:tt)*) => {
        if $crate::enabled() {
            $crate::log($target, format_args!($($arg)*))
        }
    };
}

#[inline(always)]
pub const fn enabled() -> bool {
    cfg!(feature = "video-monitor")
}

#[cfg(feature = "video-monitor")]
pub fn log(target: &str, args: std::fmt::Arguments<'_>) {
    use std::{io::Write as _, sync::OnceLock, time::Instant};

    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed();
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("worker");
    let _ = writeln!(
        std::io::stderr().lock(),
        "[monitor {:>8.3}s {name}] {target}: {args}",
        elapsed.as_secs_f64()
    );
}

#[cfg(not(feature = "video-monitor"))]
#[inline(always)]
pub fn log(_target: &str, _args: std::fmt::Arguments<'_>) {}
