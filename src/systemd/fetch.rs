//! Asynchronous data fetches, one per [`FetchKind`].

use std::sync::Arc;

use eyre::{Context, Result};
use tracing::debug;

use super::{Backend, unit::Unit};
use crate::store::{ManagerInfo, ViewData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FetchKind {
    Units,
    UnitFiles,
    Manager,
}

impl FetchKind {
    pub async fn run(self, backend: Arc<Backend>) -> Result<ViewData> {
        let started = std::time::Instant::now();
        let data = match self {
            Self::Units => {
                let units = backend.manager.list_units().await.context("ListUnits")?;
                let mut units: Vec<Unit> = units.into_iter().map(Unit::from_listed).collect();
                units.sort_by(|a, b| a.name.cmp(&b.name));
                ViewData::Units(units)
            }
            Self::UnitFiles => {
                let files = backend.manager.list_unit_files().await.context("ListUnitFiles")?;
                let map = files
                    .into_iter()
                    .map(|(path, state)| {
                        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                        (name, state)
                    })
                    .collect();
                ViewData::UnitFiles(map)
            }
            Self::Manager => {
                let m = &backend.manager;
                ViewData::Manager(ManagerInfo {
                    version: m.version().await.context("Version")?,
                    state: m.system_state().await.context("SystemState")?,
                    n_names: m.n_names().await.context("NNames")?,
                    n_failed: m.n_failed_units().await.context("NFailedUnits")?,
                    n_jobs: m.n_jobs().await.context("NJobs")?,
                })
            }
        };
        debug!(kind = ?self, elapsed = ?started.elapsed(), "fetched");
        Ok(data)
    }
}
