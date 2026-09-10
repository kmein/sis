//! Turn D-Bus errors into one-line advice.

use zbus::Error;

/// A short, actionable message for the status line.
pub fn friendly(err: &Error) -> String {
    match err {
        Error::MethodError(name, detail, _) => {
            let detail = detail.as_deref().unwrap_or("").trim_end_matches('.');
            match name.as_str() {
                "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired"
                | "org.freedesktop.DBus.Error.AccessDenied" => {
                    "permission denied (polkit org.freedesktop.systemd1.manage-units): run as root or use --user".into()
                }
                "org.freedesktop.systemd1.NoSuchUnit" => format!("no such unit: {detail}"),
                "org.freedesktop.systemd1.UnitMasked" => format!("unit is masked: {detail}"),
                "org.freedesktop.systemd1.LoadFailed" => format!("unit failed to load: {detail}"),
                "org.freedesktop.systemd1.NoSuchJob" => "the job is already gone".into(),
                "org.freedesktop.systemd1.JobTypeNotApplicable" => format!("that job type does not apply: {detail}"),
                "org.freedesktop.DBus.Error.FileNotFound" => format!("file not found: {detail}"),
                "org.freedesktop.DBus.Error.UnknownMethod" | "org.freedesktop.DBus.Error.UnknownObject" => {
                    format!("systemd does not support that here: {detail}")
                }
                other => {
                    let short = other.rsplit('.').next().unwrap_or(other);
                    if detail.is_empty() { short.to_owned() } else { format!("{short}: {detail}") }
                }
            }
        }
        Error::InputOutput(io) => format!("bus connection lost: {io}"),
        other => other.to_string(),
    }
}
