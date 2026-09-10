//! Cached data shared by all views, filled by fetches.

use std::collections::HashMap;

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
    Users(Vec<UserRow>),
    Seats(Vec<SeatRow>),
    Machines(Vec<MachineRow>),
    Links(Vec<LinkRow>),
    Boot(Vec<BootRow>),
    Security(Vec<SecurityRow>),
    Blame(Vec<BlameRow>),
    Bus(Vec<BusRow>),
    Userdb(Vec<UserdbRow>),
    Groups(Vec<GroupRow>),
    Plot(Vec<PlotRow>),
}

/// One unit of `systemd-analyze plot --json=short`; times in µs since boot.
#[derive(Debug, Clone, Deserialize)]
pub struct PlotRow {
    pub name: String,
    #[serde(default)]
    pub activating: Option<u64>,
    #[serde(default)]
    pub activated: Option<u64>,
    #[serde(default)]
    pub time: Option<u64>,
    #[serde(default)]
    pub deactivated: Option<u64>,
}

/// `loginctl list-users --json=short`.
#[derive(Debug, Clone, Deserialize)]
pub struct UserRow {
    pub uid: u32,
    pub user: String,
    #[serde(default)]
    pub linger: bool,
    #[serde(default)]
    pub state: String,
}

/// `loginctl list-seats --json=short`.
#[derive(Debug, Clone, Deserialize)]
pub struct SeatRow {
    pub seat: String,
}

/// `machinectl list --output=json`.
#[derive(Debug, Clone, Deserialize)]
pub struct MachineRow {
    pub machine: String,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub addresses: Option<serde_json::Value>,
}

/// One interface of `networkctl list --json=short`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LinkRow {
    pub index: u32,
    pub name: String,
    #[serde(default, rename = "Type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub operational_state: Option<String>,
    #[serde(default)]
    pub administrative_state: Option<String>,
    #[serde(default)]
    pub carrier_state: Option<String>,
    #[serde(default)]
    pub online_state: Option<String>,
    #[serde(default, rename = "MTU")]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub network_file: Option<String>,
}

/// `bootctl list --json=short`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootRow {
    pub id: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub show_title: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_selected: bool,
    #[serde(default)]
    pub is_reported: bool,
    /// Everything else, for the detail text.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `systemd-analyze security --json=short`.
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityRow {
    pub unit: String,
    #[serde(default)]
    pub exposure: String,
    #[serde(default)]
    pub predicate: String,
    #[serde(default)]
    pub happy: String,
}

/// One line of `systemd-analyze blame`.
#[derive(Debug, Clone)]
pub struct BlameRow {
    pub unit: String,
    pub usec: u64,
    pub text: String,
}

/// `busctl list --json=short`.
#[derive(Debug, Clone, Deserialize)]
pub struct BusRow {
    pub name: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub process: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `userdbctl user --json=short`, one object per line.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserdbRow {
    pub user_name: String,
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub gid: u32,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub home_directory: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub disposition: Option<String>,
}

/// `userdbctl group --json=short`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub group_name: String,
    #[serde(default)]
    pub gid: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub disposition: Option<String>,
}

/// `systemctl list-timers --output=json`.
#[derive(Debug, Clone, Deserialize)]
pub struct TimerRow {
    #[serde(default)]
    pub next: Option<u64>,
    #[serde(default)]
    pub last: Option<u64>,
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
    Users,
    Seats,
    Machines,
    Links,
    Boot,
    Security,
    Blame,
    Bus,
    Userdb,
    Groups,
    Plot,
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
            Self::Users(_) => DataKind::Users,
            Self::Seats(_) => DataKind::Seats,
            Self::Machines(_) => DataKind::Machines,
            Self::Links(_) => DataKind::Links,
            Self::Boot(_) => DataKind::Boot,
            Self::Security(_) => DataKind::Security,
            Self::Blame(_) => DataKind::Blame,
            Self::Bus(_) => DataKind::Bus,
            Self::Userdb(_) => DataKind::Userdb,
            Self::Groups(_) => DataKind::Groups,
            Self::Plot(_) => DataKind::Plot,
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
    pub users: Vec<UserRow>,
    pub seats: Vec<SeatRow>,
    pub machines: Vec<MachineRow>,
    pub links: Vec<LinkRow>,
    pub boot: Vec<BootRow>,
    /// Unit name → security row.
    pub security: HashMap<String, SecurityRow>,
    pub blame: Vec<BlameRow>,
    pub bus: Vec<BusRow>,
    pub userdb: Vec<UserdbRow>,
    pub groups: Vec<GroupRow>,
    pub plot: Vec<PlotRow>,
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
            ViewData::Users(rows) => self.users = rows,
            ViewData::Seats(rows) => self.seats = rows,
            ViewData::Machines(rows) => self.machines = rows,
            ViewData::Links(rows) => self.links = rows,
            ViewData::Boot(rows) => self.boot = rows,
            ViewData::Security(rows) => {
                self.security = rows.into_iter().map(|r| (r.unit.clone(), r)).collect()
            }
            ViewData::Blame(rows) => self.blame = rows,
            ViewData::Bus(rows) => self.bus = rows,
            ViewData::Userdb(rows) => self.userdb = rows,
            ViewData::Groups(rows) => self.groups = rows,
            ViewData::Plot(rows) => self.plot = rows,
        }
    }

    /// Drop everything that belongs to the previous manager.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
