use std::time::{SystemTime, UNIX_EPOCH};

pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = value as f64;
    let mut unit = 0;

    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn bytes_rate(bytes: f64) -> String {
    format!("{}/s", self::bytes(bytes.max(0.0) as u64))
}

pub fn percent(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}%")
    } else if value >= 10.0 {
        format!("{value:.1}%")
    } else {
        format!("{value:.2}%")
    }
}

pub fn number(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

pub fn duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{days}d {hours:02}h")
    } else if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

pub fn epoch_time(seconds: u64) -> String {
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "-".to_string();
    };
    if seconds > now.as_secs() {
        return "future".to_string();
    }
    format!("{} ago", duration(now.as_secs() - seconds))
}

pub fn truncate_middle(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }

    let left_len = (width - 1) / 2;
    let right_len = width - 1 - left_len;
    let left: String = value.chars().take(left_len).collect();
    let right: String = value
        .chars()
        .rev()
        .take(right_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{left}.{right}")
}

#[cfg(test)]
mod tests {
    use super::{bytes, duration, percent, truncate_middle};

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(42), "42 B");
        assert_eq!(bytes(1_500), "1.50 KB");
        assert_eq!(bytes(12_500_000), "12.5 MB");
    }

    #[test]
    fn formats_duration_compactly() {
        assert_eq!(duration(59), "00:59");
        assert_eq!(duration(3_661), "01:01:01");
        assert_eq!(duration(90_000), "1d 01h");
    }

    #[test]
    fn formats_percent_by_scale() {
        assert_eq!(percent(3.456), "3.46%");
        assert_eq!(percent(32.11), "32.1%");
        assert_eq!(percent(120.5), "120%");
    }

    #[test]
    fn truncates_middle() {
        assert_eq!(truncate_middle("abcdef", 6), "abcdef");
        assert_eq!(truncate_middle("abcdefghij", 7), "abc.hij");
    }
}
