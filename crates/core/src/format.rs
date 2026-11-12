use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rigili_bilibili::ApiError;

pub fn api_error_message(error: &ApiError) -> String {
    if error.is_login_required() {
        "登录已过期，请重新登录".into()
    } else if error.is_risk_control() {
        "请求过于频繁，请稍后再试".into()
    } else {
        error.to_string()
    }
}

pub fn format_count(value: u64) -> String {
    if value >= 100_000_000 {
        format!("{:.1}亿", value as f64 / 100_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1}万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

pub fn format_duration(seconds: u64) -> String {
    clock(Duration::from_secs(seconds), true)
}

pub fn format_clock(seconds: f64) -> String {
    clock(
        Duration::try_from_secs_f64(seconds).unwrap_or_default(),
        false,
    )
}

pub fn format_date(unix: i64) -> String {
    let (_, month, day) = civil_from_days(beijing_day(unix));
    format!("{month}-{day}")
}

pub fn format_ymd(unix: i64) -> String {
    let (year, month, day) = civil_from_days(beijing_day(unix));
    format!("{year}-{month:02}-{day:02}")
}

pub fn format_datetime(unix: i64) -> String {
    let (year, month, day) = civil_from_days(beijing_day(unix));
    let seconds = (unix + BEIJING_OFFSET).rem_euclid(86_400);
    format!(
        "{year}-{month:02}-{day:02} {:02}:{:02}",
        seconds / 3600,
        seconds % 3600 / 60
    )
}

pub fn format_message_time(unix: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format_message_time_at(unix, now)
}

fn format_message_time_at(unix: i64, now: i64) -> String {
    if unix <= 0 {
        return String::new();
    }

    let day = beijing_day(unix);
    let today = beijing_day(now);
    let (year, month, date) = civil_from_days(day);
    if day == today {
        let seconds = (unix + BEIJING_OFFSET).rem_euclid(86_400);
        format!("{:02}:{:02}", seconds / 3600, seconds % 3600 / 60)
    } else if day + 1 == today {
        "昨天".into()
    } else {
        let (current_year, _, _) = civil_from_days(today);
        if year == current_year {
            format!("{month}月{date}日")
        } else {
            format!("{year}年{month}月{date}日")
        }
    }
}

const BEIJING_OFFSET: i64 = 8 * 3600;

fn beijing_day(unix: i64) -> i64 {
    (unix + BEIJING_OFFSET).div_euclid(86_400)
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
    use super::{
        api_error_message, format_clock, format_count, format_date, format_datetime,
        format_duration, format_message_time_at, format_ymd,
    };
    use rigili_bilibili::ApiError;

    #[test]
    fn explains_login_and_rate_limit_errors() {
        let login = ApiError::Bilibili {
            code: -101,
            message: "账号未登录".into(),
        };
        assert_eq!(api_error_message(&login), "登录已过期，请重新登录");

        let risk = ApiError::Bilibili {
            code: -352,
            message: "风控校验失败".into(),
        };
        assert_eq!(api_error_message(&risk), "请求过于频繁，请稍后再试");

        let other = ApiError::Bilibili {
            code: -400,
            message: "请求错误".into(),
        };
        assert!(api_error_message(&other).contains("-400"));
    }

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
        assert_eq!(format_ymd(1_762_646_400), "2025-11-09");
        assert_eq!(format_datetime(1_762_646_400), "2025-11-09 08:00");
        assert_eq!(
            format_datetime(1_762_646_400 + 13 * 3600 + 16 * 60),
            "2025-11-09 21:16"
        );
    }

    #[test]
    fn formats_message_times() {
        let now = 1_762_646_400;
        assert_eq!(format_message_time_at(now, now), "08:00");
        assert_eq!(format_message_time_at(now - 86_400, now), "昨天");
        assert_eq!(format_message_time_at(now - 39 * 86_400, now), "10月1日");
        assert_eq!(
            format_message_time_at(now - 365 * 86_400, now),
            "2024年11月9日"
        );
        assert_eq!(format_message_time_at(0, now), "");
    }
}
