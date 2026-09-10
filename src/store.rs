//! Cached data shared by all views, filled by fetches.

use std::collections::HashMap;

use crate::systemd::unit::{Enrichment, Unit};

/// One dataset delivered by a fetch; each variant fills one slot of [`Store`].
#[derive(Debug)]
pub enum ViewData {
    Units(Vec<Unit>),
    /// Unit file name (basename) → enablement state.
    UnitFiles(HashMap<String, String>),
    Manager(ManagerInfo),
    Enrichment(Vec<(String, Enrichment)>),
}

/// Which slot a [`ViewData`] fills; what views get told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Units,
    UnitFiles,
    Manager,
    Enrichment,
}

impl ViewData {
    pub fn kind(&self) -> DataKind {
        match self {
            Self::Units(_) => DataKind::Units,
            Self::UnitFiles(_) => DataKind::UnitFiles,
            Self::Manager(_) => DataKind::Manager,
            Self::Enrichment(_) => DataKind::Enrichment,
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
}

impl Store {
    /// Absorb one dataset.
    pub fn apply(&mut self, data: ViewData) {
        match data {
            ViewData::Units(units) => self.units = units,
            ViewData::UnitFiles(files) => self.unit_files = files,
            ViewData::Manager(info) => self.manager = info,
            ViewData::Enrichment(items) => self.enrichment.extend(items),
        }
    }

    /// Drop everything that belongs to the previous manager.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
