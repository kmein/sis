//! Journal tailing through a `journalctl -o json -f` subprocess.

use std::{
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
};

use eyre::{Context, Result};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc::Sender,
    task::JoinHandle,
};
use tracing::{debug, warn};

use super::Scope;
use crate::event::Event;

/// Identifies one running tail; views own one each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JournalId(u64);

impl JournalId {
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalTarget {
    Unit(String),
    Pid(u32),
    Machine(String),
    All,
}

#[derive(Debug, Clone)]
pub struct JournalSpec {
    pub scope: Scope,
    pub target: JournalTarget,
    /// How many recent lines to start with.
    pub lines: usize,
}

impl JournalSpec {
    pub fn unit(scope: Scope, unit: &str) -> Self {
        Self {
            scope,
            target: JournalTarget::Unit(unit.to_owned()),
            lines: 500,
        }
    }

    /// The journalctl arguments for this spec.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            "--no-pager".to_owned(),
            "-o".into(),
            "json".into(),
            "-f".into(),
            format!("-n{}", self.lines),
        ];
        match &self.target {
            JournalTarget::Unit(u) => match self.scope {
                Scope::System => args.push(format!("--unit={u}")),
                Scope::User => args.push(format!("--user-unit={u}")),
            },
            JournalTarget::Pid(pid) => args.push(format!("_PID={pid}")),
            JournalTarget::Machine(m) => args.push(format!("--machine={m}")),
            JournalTarget::All => {
                if self.scope == Scope::User {
                    args.push("--user".into());
                }
            }
        }
        args
    }

    pub fn describe(&self) -> String {
        match &self.target {
            JournalTarget::Unit(u) => u.clone(),
            JournalTarget::Pid(p) => format!("pid {p}"),
            JournalTarget::Machine(m) => format!("machine {m}"),
            JournalTarget::All => format!("{} journal", self.scope),
        }
    }
}

/// One parsed journal record.
#[derive(Debug, Clone)]
pub struct JournalEntry {
    /// µs since the epoch.
    pub timestamp: u64,
    pub priority: u8,
    pub message: String,
    pub identifier: String,
    pub pid: Option<u32>,
}

impl JournalEntry {
    /// Parse one line of `journalctl -o json`. Every numeric field is a
    /// string there, and `MESSAGE` may be null (too large) or a byte array
    /// (not valid UTF-8).
    pub fn parse(line: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(line).ok()?;
        let obj = v.as_object()?;
        let str_field = |k: &str| obj.get(k).and_then(Value::as_str).map(str::to_owned);
        let num_field = |k: &str| {
            obj.get(k)
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
        };
        let message = match obj.get("MESSAGE") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(bytes)) => {
                let raw: Vec<u8> = bytes
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|b| b as u8)
                    .collect();
                String::from_utf8_lossy(&raw).into_owned()
            }
            Some(Value::Null) => "[message too large; see journalctl --all]".to_owned(),
            _ => String::new(),
        };
        Some(Self {
            timestamp: num_field("__REALTIME_TIMESTAMP").unwrap_or(0),
            priority: num_field("PRIORITY").map_or(6, |p| p.min(7) as u8),
            message,
            identifier: str_field("SYSLOG_IDENTIFIER")
                .or_else(|| str_field("_COMM"))
                .unwrap_or_default(),
            pid: num_field("_PID").map(|p| p as u32),
        })
    }

    pub fn is_from_systemd(&self) -> bool {
        self.identifier == "systemd" || self.identifier.starts_with("systemd-")
    }
}

/// What a tail sends to its view.
#[derive(Debug, Clone)]
pub enum JournalItem {
    Entry(JournalEntry),
    /// The process ended; `Some` carries its stderr when it failed.
    Ended(Option<String>),
}

/// A running `journalctl`; dropping it kills the process.
pub struct JournalHandle {
    child: Child,
    reader: JoinHandle<()>,
}

impl JournalHandle {
    pub fn stop(mut self) {
        self.reader.abort();
        let _ = self.child.start_kill();
    }
}

pub fn spawn(id: JournalId, spec: &JournalSpec, tx: Sender<Event>) -> Result<JournalHandle> {
    let args = spec.args();
    debug!(?id, ?args, "spawning journalctl");
    let mut child = Command::new("journalctl")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("spawning journalctl")?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if let Some(entry) = JournalEntry::parse(&line)
                        && tx
                            .send(Event::Journal {
                                id,
                                item: JournalItem::Entry(entry),
                            })
                            .await
                            .is_err()
                    {
                        return;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    warn!(%err, "journalctl stdout");
                    break;
                }
            }
        }
        let mut err_text = String::new();
        let mut err_lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = err_lines.next_line().await {
            err_text.push_str(l.trim());
            err_text.push(' ');
        }
        let error = (!err_text.trim().is_empty()).then(|| err_text.trim().to_owned());
        let _ = tx
            .send(Event::Journal {
                id,
                item: JournalItem::Ended(error),
            })
            .await;
    });
    Ok(JournalHandle { child, reader })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_line() {
        let line = r#"{"PRIORITY":"4","MESSAGE":"hello","__REALTIME_TIMESTAMP":"1789020000000000","_PID":"42","SYSLOG_IDENTIFIER":"sis"}"#;
        let e = JournalEntry::parse(line).unwrap();
        assert_eq!(e.priority, 4);
        assert_eq!(e.message, "hello");
        assert_eq!(e.pid, Some(42));
        assert_eq!(e.identifier, "sis");
        assert_eq!(e.timestamp, 1_789_020_000_000_000);
    }

    #[test]
    fn parses_byte_array_message() {
        let line = r#"{"MESSAGE":[104,105,255],"PRIORITY":"6"}"#;
        let e = JournalEntry::parse(line).unwrap();
        assert!(e.message.starts_with("hi"));
    }

    #[test]
    fn user_scope_uses_user_unit() {
        let spec = JournalSpec::unit(Scope::User, "foo.service");
        assert!(spec.args().contains(&"--user-unit=foo.service".to_owned()));
        let spec = JournalSpec::unit(Scope::System, "foo.service");
        assert!(spec.args().contains(&"--unit=foo.service".to_owned()));
    }
}
