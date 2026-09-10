//! Cached data shared by all views, filled by fetches.

use std::{collections::HashMap, time::Instant};

use serde::Deserialize;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::systemd::{
    types::Process,
    unit::{Enrichment, Unit},
};

/// One dataset delivered by a fetch; each variant fills one slot of [`Store`].
#[derive(Debug)]
pub enum ViewData {
    Units(Vec<Unit>),
    /// Unit file name (basename) → enablement state.
    UnitFiles(HashMap<String, String>),
    Manager(ManagerInfo),
    Enrichment(Vec<(String, Enrichment)>),
    UnitDetail(Box<UnitDetail>),
    Timers(Vec<TimerRow>),
    Sockets(Vec<SocketRow>),
    Jobs(Vec<JobRow>),
    Coredumps(Vec<CoredumpRow>),
    Sessions(Vec<SessionRow>),
}

/// `systemctl list-timers --output=json`.
#[derive(Debug, Clone, Deserialize)]
pub struct TimerRow {
    #[serde(default)]
    pub next: Option<u64>,
    #[serde(default)]
    pub left: Option<u64>,
    #[serde(default)]
    pub last: Option<u64>,
    #[serde(default)]
    pub passed: Option<u64>,
    pub unit: String,
    #[serde(default)]
    pub activates: Option<String>,
}

/// `systemctl list-sockets --output=json`; one row per listen address.
#[derive(Debug, Clone, Deserialize)]
pub struct SocketRow {
    pub listen: String,
    pub unit: String,
    #[serde(default)]
    pub activates: Option<String>,
    /// `unit listen`, filled after parsing.
    #[serde(skip)]
    pub key: String,
}

/// `Manager.ListJobs`.
#[derive(Debug, Clone)]
pub struct JobRow {
    pub id: u32,
    pub unit: String,
    pub kind: String,
    pub state: String,
    pub key: String,
}

/// `coredumpctl list --json=short`.
#[derive(Debug, Clone, Deserialize)]
pub struct CoredumpRow {
    #[serde(default)]
    pub time: u64,
    pub pid: u32,
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub gid: u32,
    #[serde(default)]
    pub sig: Option<i64>,
    #[serde(default)]
    pub corefile: Option<String>,
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(skip)]
    pub key: String,
}

/// `loginctl list-sessions --json=short`.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionRow {
    pub session: String,
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub seat: Option<String>,
    #[serde(default)]
    pub leader: Option<u32>,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub tty: Option<String>,
    #[serde(default)]
    pub idle: bool,
    #[serde(default)]
    pub since: Option<u64>,
}

/// Everything the detail view shows for one unit.
#[derive(Debug, Clone)]
pub struct UnitDetail {
    pub name: String,
    pub path: OwnedObjectPath,
    /// `org.freedesktop.systemd1.Unit` properties.
    pub unit: HashMap<String, OwnedValue>,
    /// Properties of the type-specific interface (Service, Timer, ...).
    pub typed: HashMap<String, OwnedValue>,
    pub processes: Vec<Process>,
    /// Unit file and drop-ins: path and contents (or the read error).
    pub files: Vec<(String, Result<String, String>)>,
    pub fetched_at: Instant,
}

/// Which slot a [`ViewData`] fills; what views get told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Units,
    UnitFiles,
    Manager,
    Enrichment,
    UnitDetail,
    Timers,
    Sockets,
    Jobs,
    Coredumps,
    Sessions,
}

impl ViewData {
    pub fn kind(&self) -> DataKind {
        match self {
            Self::Units(_) => DataKind::Units,
            Self::UnitFiles(_) => DataKind::UnitFiles,
            Self::Manager(_) => DataKind::Manager,
            Self::Enrichment(_) => DataKind::Enrichment,
            Self::UnitDetail(_) => DataKind::UnitDetail,
            Self::Timers(_) => DataKind::Timers,
            Self::Sockets(_) => DataKind::Sockets,
            Self::Jobs(_) => DataKind::Jobs,
            Self::Coredumps(_) => DataKind::Coredumps,
            Self::Sessions(_) => DataKind::Sessions,
        }
    }
}

/// Headline figures of the service manager for the header.
#[derive(Debug, Clone, Default)]
pub struct ManagerInfo {
    pub version: String,
    pub state: String,
    pub n_names: u32,
    pub n_failed: u32,
    pub n_jobs: u32,
}

#[derive(Debug, Default)]
pub struct Store {
    pub units: Vec<Unit>,
    pub unit_files: HashMap<String, String>,
    pub manager: ManagerInfo,
    pub enrichment: HashMap<String, Enrichment>,
    /// Latest detail per unit name.
    pub detail: HashMap<String, UnitDetail>,
    pub timers: Vec<TimerRow>,
    pub sockets: Vec<SocketRow>,
    pub jobs: Vec<JobRow>,
    pub coredumps: Vec<CoredumpRow>,
    pub sessions: Vec<SessionRow>,
}

impl Store {
    /// The object path of a loaded unit, if we have seen it.
    pub fn unit_path(&self, name: &str) -> Option<OwnedObjectPath> {
        self.units
            .iter()
            .find(|u| u.name == name)
            .map(|u| u.path.clone())
    }

    /// Absorb one dataset.
    pub fn apply(&mut self, data: ViewData) {
        match data {
            ViewData::Units(units) => self.units = units,
            ViewData::UnitFiles(files) => self.unit_files = files,
            ViewData::Manager(info) => self.manager = info,
            ViewData::Enrichment(items) => self.enrichment.extend(items),
            ViewData::UnitDetail(detail) => {
                self.detail.insert(detail.name.clone(), *detail);
            }
            ViewData::Timers(rows) => self.timers = rows,
            ViewData::Sockets(rows) => self.sockets = rows,
            ViewData::Jobs(rows) => self.jobs = rows,
            ViewData::Coredumps(rows) => self.coredumps = rows,
            ViewData::Sessions(rows) => self.sessions = rows,
        }
    }

    /// Drop everything that belongs to the previous manager.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
