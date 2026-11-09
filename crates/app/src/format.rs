use std::time::Duration;

pub(crate) fn format_count(value: u64) -> String {
    if value >= 100_000_000 {
        format!("{:.1}亿", value as f64 / 100_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1}万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

pub(crate) fn format_duration(seconds: u64) -> String {
    clock(Duration::from_secs(seconds), true)
}

pub(crate) fn format_clock(seconds: f64) -> String {
    clock(
        Duration::try_from_secs_f64(seconds).unwrap_or_default(),
        false,
    )
}

pub(crate) fn format_date(unix: i64) -> String {
    let (_, month, day) = civil_from_days(unix.div_euclid(86_400));
    format!("{month}-{day}")
}

fn clock(duration: Duration, pad_minutes: bool) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = total % 3600 / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else if pad_minutes {
        format!("{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::{format_clock, format_count, format_date, format_duration};

    #[test]
    fn formats_counts() {
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(12_345), "1.2万");
        assert_eq!(format_count(123_456_789), "1.2亿");
    }

    #[test]
    fn formats_list_durations() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(156), "02:36");
        assert_eq!(format_duration(3_600), "1:00:00");
    }

    #[test]
    fn formats_playback_clock() {
        assert_eq!(format_clock(0.0), "0:00");
        assert_eq!(format_clock(65.4), "1:05");
        assert_eq!(format_clock(65.9), "1:05");
        assert_eq!(format_clock(3_661.0), "1:01:01");
        assert_eq!(format_clock(f64::NAN), "0:00");
        assert_eq!(format_clock(f64::INFINITY), "0:00");
        assert_eq!(format_clock(-3.0), "0:00");
    }

    #[test]
    fn formats_publish_dates() {
        assert_eq!(format_date(1_762_646_400), "11-9");
    }
}
