//! Human formatting of systemd's raw numbers.

use jiff::{Timestamp, tz::TimeZone};

/// Format a µs-since-epoch timestamp in local time; empty for 0 (never).
pub fn timestamp(usec: u64) -> String {
    if usec == 0 {
        return String::new();
    }
    match Timestamp::from_microsecond(usec as i64) {
        Ok(ts) => ts
            .to_zoned(TimeZone::system())
            .strftime("%Y-%m-%d %H:%M:%S %Z")
            .to_string(),
        Err(_) => String::new(),
    }
}

/// k9s-style age of a µs-since-epoch timestamp: `12s`, `5m`, `3h`, `2d4h`.
pub fn age(usec: u64) -> String {
    if usec == 0 {
        return String::new();
    }
    let now = Timestamp::now().as_microsecond();
    let secs = (now - usec as i64).max(0) / 1_000_000;
    duration_secs(secs as u64)
}

/// Compact duration.
pub fn duration_secs(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        secs % 86_400 / 3_600,
        secs % 3_600 / 60,
        secs % 60,
    );
    if d > 0 {
        if h > 0 {
            format!("{d}d{h}h")
        } else {
            format!("{d}d")
        }
    } else if h > 0 {
        if m > 0 {
            format!("{h}h{m}m")
        } else {
            format!("{h}h")
        }
    } else if m > 0 {
        if s > 0 && m < 10 {
            format!("{m}m{s}s")
        } else {
            format!("{m}m")
        }
    } else {
        format!("{s}s")
    }
}

/// Bytes in IEC units, one decimal above KiB.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n}B")
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration_secs(5), "5s");
        assert_eq!(duration_secs(65), "1m5s");
        assert_eq!(duration_secs(15 * 60), "15m");
        assert_eq!(duration_secs(3_600 * 3 + 120), "3h2m");
        assert_eq!(duration_secs(86_400 * 2 + 3_600 * 4), "2d4h");
    }

    #[test]
    fn byte_sizes() {
        assert_eq!(bytes(512), "512B");
        assert_eq!(bytes(9_580_544), "9.1M");
    }
}
