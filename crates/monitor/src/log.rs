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
    tracing::info!(target: "rigili", "{target}: {args}");
}

#[cfg(not(feature = "video-monitor"))]
#[inline(always)]
pub fn log(_target: &str, _args: std::fmt::Arguments<'_>) {}
