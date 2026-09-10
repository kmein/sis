//! Mutating operations on units, mapped onto the Manager interface.

use std::fmt;

use zbus::zvariant::OwnedObjectPath;

use super::Backend;

const SIGTERM: i32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitAction {
    Start,
    Stop,
    Restart,
    Reload,
    ReloadOrRestart,
    Enable,
    Disable,
    Mask,
    Unmask,
    ResetFailed,
    Kill,
    CancelJob(u32),
    DaemonReload,
}

impl UnitAction {
    /// Present participle for "restarting nginx.service…".
    pub fn progressive(&self) -> &'static str {
        match self {
            Self::Start => "starting",
            Self::Stop => "stopping",
            Self::Restart => "restarting",
            Self::Reload | Self::ReloadOrRestart => "reloading",
            Self::Enable => "enabling",
            Self::Disable => "disabling",
            Self::Mask => "masking",
            Self::Unmask => "unmasking",
            Self::ResetFailed => "resetting",
            Self::Kill => "killing",
            Self::CancelJob(_) => "cancelling job of",
            Self::DaemonReload => "reloading",
        }
    }
}

impl fmt::Display for UnitAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Reload => "reload",
            Self::ReloadOrRestart => "reload-or-restart",
            Self::Enable => "enable",
            Self::Disable => "disable",
            Self::Mask => "mask",
            Self::Unmask => "unmask",
            Self::ResetFailed => "reset-failed",
            Self::Kill => "kill",
            Self::CancelJob(_) => "cancel",
            Self::DaemonReload => "daemon-reload",
        })
    }
}

/// What an action produced: a job to wait for, or an immediate result.
pub enum Outcome {
    Job(OwnedObjectPath),
    Done(String),
}

/// Run one action. Errors are already made friendly.
pub async fn perform(backend: &Backend, action: UnitAction, unit: &str) -> Result<Outcome, String> {
    let m = &backend.manager;
    let job = |r: zbus::Result<OwnedObjectPath>| {
        r.map(Outcome::Job).map_err(|e| super::errors::friendly(&e))
    };
    let done = |r: zbus::Result<()>, msg: String| {
        r.map(|()| Outcome::Done(msg))
            .map_err(|e| super::errors::friendly(&e))
    };
    match action {
        UnitAction::Start => job(m.start_unit(unit, "replace").await),
        UnitAction::Stop => job(m.stop_unit(unit, "replace").await),
        UnitAction::Restart => job(m.restart_unit(unit, "replace").await),
        UnitAction::Reload => job(m.reload_unit(unit, "replace").await),
        UnitAction::ReloadOrRestart => job(m.reload_or_restart_unit(unit, "replace").await),
        UnitAction::ResetFailed => done(m.reset_failed_unit(unit).await, format!("reset {unit}")),
        UnitAction::Kill => done(
            m.kill_unit(unit, "all", SIGTERM).await,
            format!("sent SIGTERM to {unit}"),
        ),
        UnitAction::CancelJob(id) => done(m.cancel_job(id).await, format!("cancelled job {id}")),
        UnitAction::DaemonReload => done(m.reload().await, "daemon reloaded".into()),
        UnitAction::Enable | UnitAction::Disable | UnitAction::Mask | UnitAction::Unmask => {
            let files = [unit];
            let changes = match action {
                UnitAction::Enable => m
                    .enable_unit_files(&files, false, false)
                    .await
                    .map(|(_, c)| c),
                UnitAction::Disable => m.disable_unit_files(&files, false).await,
                UnitAction::Mask => m.mask_unit_files(&files, false, false).await,
                _ => m.unmask_unit_files(&files, false).await,
            }
            .map_err(|e| super::errors::friendly(&e))?;
            // Like systemctl, make the change visible to the manager right away.
            m.reload().await.map_err(|e| super::errors::friendly(&e))?;
            let n = changes.len();
            let what = match action {
                UnitAction::Enable => "enabled",
                UnitAction::Disable => "disabled",
                UnitAction::Mask => "masked",
                _ => "unmasked",
            };
            Ok(Outcome::Done(if n == 0 {
                format!("{unit} already {what}, nothing to do")
            } else {
                format!(
                    "{what} {unit} ({n} symlink change{})",
                    if n == 1 { "" } else { "s" }
                )
            }))
        }
    }
}
