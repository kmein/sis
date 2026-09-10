//! Cached data shared by all views, filled by fetches.

use std::{collections::HashMap, time::Instant};

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
}

impl ViewData {
    pub fn kind(&self) -> DataKind {
        match self {
            Self::Units(_) => DataKind::Units,
            Self::UnitFiles(_) => DataKind::UnitFiles,
            Self::Manager(_) => DataKind::Manager,
            Self::Enrichment(_) => DataKind::Enrichment,
            Self::UnitDetail(_) => DataKind::UnitDetail,
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
}

impl Store {
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
        }
    }

    /// Drop everything that belongs to the previous manager.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
