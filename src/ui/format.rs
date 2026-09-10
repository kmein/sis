//! Human formatting of systemd's raw numbers.

use jiff::{Timestamp, tz::TimeZone};
use zbus::zvariant::Value;

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

/// Time left until a µs-since-epoch timestamp, compact.
pub fn until(usec: u64) -> String {
    if usec == 0 {
        return String::new();
    }
    let now = Timestamp::now().as_microsecond();
    let secs = (usec as i64 - now).max(0) / 1_000_000;
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

/// Render a D-Bus value the way `systemctl show` roughly would.
pub fn value(v: &Value<'_>) -> String {
    match v {
        Value::U8(n) => n.to_string(),
        Value::Bool(b) => {
            if *b {
                "yes".into()
            } else {
                "no".into()
            }
        }
        Value::I16(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => {
            if *n == u64::MAX {
                "[not set]".into()
            } else {
                n.to_string()
            }
        }
        Value::F64(n) => n.to_string(),
        Value::Str(s) => s.to_string(),
        Value::Signature(s) => s.to_string(),
        Value::ObjectPath(p) => p.to_string(),
        Value::Value(inner) => value(inner),
        Value::Array(a) => {
            let items: Vec<&Value> = a.iter().collect();
            if items.iter().all(|i| matches!(i, Value::U8(_))) && !items.is_empty() {
                items
                    .iter()
                    .map(|i| match i {
                        Value::U8(b) => format!("{b:02x}"),
                        _ => String::new(),
                    })
                    .collect()
            } else {
                items.iter().map(|i| value(i)).collect::<Vec<_>>().join(" ")
            }
        }
        Value::Dict(d) => d
            .iter()
            .map(|(k, v)| format!("{}={}", value(k), value(v)))
            .collect::<Vec<_>>()
            .join(" "),
        Value::Structure(s) => {
            format!(
                "({})",
                s.fields().iter().map(value).collect::<Vec<_>>().join(", ")
            )
        }
        #[cfg(unix)]
        Value::Fd(_) => "<fd>".into(),
    }
}

/// Nanoseconds of CPU time as `1.234s`, `2min 3.4s`, ...
pub fn cpu_ns(ns: u64) -> String {
    let secs = ns as f64 / 1e9;
    if secs < 60.0 {
        format!("{secs:.3}s")
    } else if secs < 3600.0 {
        format!("{}min {:.1}s", (secs / 60.0) as u64, secs % 60.0)
    } else {
        format!(
            "{}h {}min",
            (secs / 3600.0) as u64,
            ((secs % 3600.0) / 60.0) as u64
        )
    }
}
