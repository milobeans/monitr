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

pub fn signed_percent(value: f64) -> String {
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    format!("{sign}{}", percent(value.abs()))
}

pub fn signed_bytes(value: i64) -> String {
    let sign = if value > 0 {
        "+"
    } else if value < 0 {
        "-"
    } else {
        ""
    };
    format!("{sign}{}", bytes(value.unsigned_abs()))
}

pub fn signed_bytes_rate(value: f64) -> String {
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    format!("{sign}{}", bytes_rate(value.abs()))
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

/// Render values as a Unicode block sparkline scaled against `max`. Values at
/// or above `max` render as a full block; empty input yields an empty string.
pub fn sparkline(values: &[f64], max: f64) -> String {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = max.max(f64::MIN_POSITIVE);
    values
        .iter()
        .map(|value| {
            let ratio = (value / max).clamp(0.0, 1.0);
            let level = (ratio * (LEVELS.len() - 1) as f64).round() as usize;
            LEVELS[level.min(LEVELS.len() - 1)]
        })
        .collect()
}

pub fn truncate_middle(value: &str, width: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }

    let left_len = (width - 1) / 2;
    let right_len = width - 1 - left_len;
    let left: String = value.chars().take(left_len).collect();
    let right: String = value.chars().skip(char_count - right_len).collect();
    format!("{left}.{right}")
}

#[cfg(test)]
mod tests {
    use super::{
        bytes, duration, percent, signed_bytes, signed_percent, sparkline, truncate_middle,
    };

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
    fn formats_signed_values() {
        assert_eq!(signed_percent(3.456), "+3.46%");
        assert_eq!(signed_percent(-32.11), "-32.1%");
        assert_eq!(signed_bytes(1_500), "+1.50 KB");
        assert_eq!(signed_bytes(-1_500), "-1.50 KB");
    }

    #[test]
    fn renders_sparkline_against_scale() {
        assert_eq!(sparkline(&[], 100.0), "");
        assert_eq!(sparkline(&[0.0, 100.0], 100.0), "▁█");
        // Values clamp to the scale rather than overflowing the glyph range.
        assert_eq!(sparkline(&[200.0], 100.0), "█");
        assert_eq!(sparkline(&[50.0], 100.0), "▅");
    }

    #[test]
    fn truncates_middle() {
        assert_eq!(truncate_middle("abcdef", 6), "abcdef");
        assert_eq!(truncate_middle("abcdefghij", 7), "abc.hij");
    }
}
