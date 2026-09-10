//! The unit row model.

use std::time::Instant;

use zbus::zvariant::OwnedObjectPath;

use super::types::{ActiveState, ListedUnit, LoadState, UnitKind};

#[derive(Debug, Clone)]
pub struct Unit {
    pub name: String,
    pub description: String,
    pub load: LoadState,
    pub active: ActiveState,
    pub sub: String,
    pub kind: UnitKind,
    pub following: Option<String>,
    pub path: OwnedObjectPath,
    /// Queued job: id and type.
    pub job: Option<(u32, String)>,
    /// Enablement state from the unit file list, if the unit has a file.
    pub file_state: Option<String>,
    /// Lazily fetched per-unit properties.
    pub enrich: Option<Enrichment>,
}

/// Per-unit properties that need a round trip each.
#[derive(Debug, Clone)]
pub struct Enrichment {
    pub fetched_at: Instant,
    /// `ActiveEnterTimestamp` in µs since the epoch; 0 if never.
    pub active_enter: u64,
    /// `StateChangeTimestamp` in µs since the epoch; 0 if never.
    pub state_change: u64,
    pub main_pid: Option<u32>,
    pub n_restarts: Option<u32>,
    pub memory_current: Option<u64>,
    pub tasks_current: Option<u64>,
    pub need_daemon_reload: bool,
}

impl Unit {
    pub fn from_listed(u: ListedUnit) -> Self {
        let ListedUnit(name, description, load, active, sub, following, path, job_id, job_type, _job_path) = u;
        Self {
            kind: UnitKind::of(&name),
            load: LoadState::parse(&load),
            active: ActiveState::parse(&active),
            following: (!following.is_empty()).then_some(following),
            job: (job_id != 0).then_some((job_id, job_type)),
            name,
            description,
            sub,
            path,
            file_state: None,
            enrich: None,
        }
    }

    /// `systemctl list-units` without `--all` hides units that are inactive
    /// with nothing queued, and units that merely follow another one (device
    /// aliases).
    pub fn is_boring(&self) -> bool {
        (self.active == ActiveState::Inactive && self.job.is_none()) || self.following.is_some()
    }

    pub fn is_failed(&self) -> bool {
        self.active == ActiveState::Failed
    }
}
