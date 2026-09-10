//! Asynchronous data fetches, one per [`FetchKind`].

use std::sync::Arc;

use eyre::{Context, Result};
use tracing::debug;

use std::{collections::HashMap, time::Instant};

use zbus::{
    fdo::PropertiesProxy,
    names::InterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::{
    Backend,
    types::UnitKind,
    unit::{Enrichment, Unit},
};
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
                let files = backend
                    .manager
                    .list_unit_files()
                    .await
                    .context("ListUnitFiles")?;
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

/// `Properties.GetAll` on one unit object for one interface.
pub async fn properties(
    backend: &Backend,
    path: &OwnedObjectPath,
    iface: &str,
) -> Result<HashMap<String, OwnedValue>> {
    let proxy = PropertiesProxy::builder(&backend.conn)
        .destination("org.freedesktop.systemd1")?
        .path(path.clone())?
        .build()
        .await?;
    let name = InterfaceName::try_from(iface.to_owned())?;
    Ok(proxy
        .get_all(name)
        .await
        .with_context(|| format!("GetAll {iface} on {path}"))?)
}

/// Read a numeric property, treating systemd's `u64::MAX` "unset" as absent.
pub fn prop_u64(props: &HashMap<String, OwnedValue>, name: &str) -> Option<u64> {
    let v = props.get(name)?;
    let n: u64 = if let Ok(n) = u64::try_from(v.clone()) {
        n
    } else {
        u64::from(u32::try_from(v.clone()).ok()?)
    };
    (n != u64::MAX).then_some(n)
}

pub fn prop_u32(props: &HashMap<String, OwnedValue>, name: &str) -> Option<u32> {
    props.get(name).and_then(|v| u32::try_from(v.clone()).ok())
}

pub fn prop_bool(props: &HashMap<String, OwnedValue>, name: &str) -> Option<bool> {
    props.get(name).and_then(|v| bool::try_from(v.clone()).ok())
}

pub fn prop_str(props: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    props
        .get(name)
        .and_then(|v| String::try_from(v.clone()).ok())
}

/// Fetch the per-unit properties the table shows, for a batch of units.
pub async fn enrich(
    backend: Arc<Backend>,
    targets: Vec<(String, OwnedObjectPath)>,
) -> Result<ViewData> {
    let started = Instant::now();
    let mut out = Vec::with_capacity(targets.len());
    for (name, path) in targets {
        let unit = match properties(&backend, &path, "org.freedesktop.systemd1.Unit").await {
            Ok(p) => p,
            Err(err) => {
                debug!(%name, %err, "enrichment failed");
                continue;
            }
        };
        let kind = UnitKind::of(&name);
        let typed = match kind {
            UnitKind::Service => properties(&backend, &path, "org.freedesktop.systemd1.Service")
                .await
                .ok(),
            UnitKind::Scope => properties(&backend, &path, "org.freedesktop.systemd1.Scope")
                .await
                .ok(),
            UnitKind::Slice => properties(&backend, &path, "org.freedesktop.systemd1.Slice")
                .await
                .ok(),
            UnitKind::Mount => properties(&backend, &path, "org.freedesktop.systemd1.Mount")
                .await
                .ok(),
            UnitKind::Socket => properties(&backend, &path, "org.freedesktop.systemd1.Socket")
                .await
                .ok(),
            _ => None,
        };
        let typed = typed.unwrap_or_default();
        out.push((
            name,
            Enrichment {
                fetched_at: Instant::now(),
                active_enter: prop_u64(&unit, "ActiveEnterTimestamp").unwrap_or(0),
                state_change: prop_u64(&unit, "StateChangeTimestamp").unwrap_or(0),
                main_pid: prop_u32(&typed, "MainPID").or_else(|| prop_u32(&typed, "ControlPID")),
                n_restarts: prop_u32(&typed, "NRestarts"),
                memory_current: prop_u64(&typed, "MemoryCurrent"),
                tasks_current: prop_u64(&typed, "TasksCurrent"),
                need_daemon_reload: prop_bool(&unit, "NeedDaemonReload").unwrap_or(false),
            },
        ));
    }
    debug!(n = out.len(), elapsed = ?started.elapsed(), "enriched");
    Ok(ViewData::Enrichment(out))
}
