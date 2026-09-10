//! Running the other `*ctl` tools and parsing their JSON.

use std::process::Stdio;

use eyre::{Context, Result, eyre};
use serde::de::DeserializeOwned;
use tokio::process::Command;
use tracing::debug;

/// How a tool lays out its JSON output.
#[derive(Debug, Clone, Copy)]
pub enum JsonShape {
    /// A top-level array of objects.
    Array,
    /// An object whose field `.0` holds the array (networkctl).
    ObjectKey(&'static str),
    /// One object per line (userdbctl).
    Lines,
}

/// Run a command; `Ok(stdout)` on success, `Err(message)` otherwise.
pub async fn run(program: &str, args: &[String]) -> Result<String, String> {
    debug!(program, ?args, "running");
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    let text = if stderr.is_empty() {
        stdout.trim().to_owned()
    } else {
        stderr.to_owned()
    };
    Err(if text.is_empty() {
        format!("{program} failed with {}", output.status)
    } else {
        text
    })
}

/// Run a command and parse its JSON output. A tool that reports "No ..."
/// (coredumpctl with no dumps) yields an empty list instead of an error.
pub async fn run_json<T: DeserializeOwned>(
    program: &str,
    args: &[String],
    shape: JsonShape,
) -> Result<Vec<T>> {
    let stdout = match run(program, args).await {
        Ok(out) => out,
        Err(msg) if msg.starts_with("No ") => return Ok(Vec::new()),
        Err(msg) => return Err(eyre!(msg)),
    };
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }
    let items = match shape {
        JsonShape::Array => {
            serde_json::from_str(&stdout).with_context(|| format!("parsing {program} output"))?
        }
        JsonShape::ObjectKey(key) => {
            let v: serde_json::Value = serde_json::from_str(&stdout)
                .with_context(|| format!("parsing {program} output"))?;
            let arr = v
                .get(key)
                .cloned()
                .unwrap_or(serde_json::Value::Array(Vec::new()));
            serde_json::from_value(arr).with_context(|| format!("parsing {program} .{key}"))?
        }
        JsonShape::Lines => stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).with_context(|| format!("parsing a {program} line")))
            .collect::<Result<Vec<T>>>()?,
    };
    Ok(items)
}
