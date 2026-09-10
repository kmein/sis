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
use super::{
    Scope,
    subprocess::{JsonShape, run_json},
};
use crate::store::{
    CoredumpRow, JobRow, ManagerInfo, SessionRow, SocketRow, TimerRow, UnitDetail, ViewData,
};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// systemctl arguments, with `--user` when talking to the user manager.
pub fn scoped(scope: Scope, items: &[&str]) -> Vec<String> {
    let mut v = args(items);
    if scope == Scope::User {
        v.insert(0, "--user".to_owned());
    }
    v
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FetchKind {
    Units,
    UnitFiles,
    Manager,
    Detail {
        name: String,
        path: Option<OwnedObjectPath>,
    },
    Timers,
    Sockets,
    Jobs,
    Coredumps,
    Sessions,
}

impl FetchKind {
    pub async fn run(self, backend: Arc<Backend>) -> Result<ViewData> {
        let started = std::time::Instant::now();
        let data = match &self {
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
            Self::Detail { name, path } => {
                let path = match path {
                    Some(p) => p.clone(),
                    None => backend.manager.load_unit(name).await.context("LoadUnit")?,
                };
                ViewData::UnitDetail(Box::new(detail(&backend, name, &path).await?))
            }
            Self::Timers => {
                let rows: Vec<TimerRow> = run_json(
                    "systemctl",
                    &scoped(backend.scope, &["list-timers", "--all", "--output=json"]),
                    JsonShape::Array,
                )
                .await?;
                ViewData::Timers(rows)
            }
            Self::Sockets => {
                let mut rows: Vec<SocketRow> = run_json(
                    "systemctl",
                    &scoped(backend.scope, &["list-sockets", "--all", "--output=json"]),
                    JsonShape::Array,
                )
                .await?;
                for r in &mut rows {
                    r.key = format!("{} {}", r.unit, r.listen);
                }
                ViewData::Sockets(rows)
            }
            Self::Jobs => {
                let jobs = backend.manager.list_jobs().await.context("ListJobs")?;
                ViewData::Jobs(
                    jobs.into_iter()
                        .map(|j| JobRow {
                            key: j.0.to_string(),
                            id: j.0,
                            unit: j.1,
                            kind: j.2,
                            state: j.3,
                        })
                        .collect(),
                )
            }
            Self::Coredumps => {
                // Newest first, capped: hosts with a crash loop have 100k+ dumps.
                let mut rows: Vec<CoredumpRow> = run_json(
                    "coredumpctl",
                    &args(&[
                        "list",
                        "--json=short",
                        "--no-pager",
                        "--reverse",
                        "-n",
                        "2000",
                    ]),
                    JsonShape::Array,
                )
                .await?;
                for r in &mut rows {
                    r.key = format!("{}:{}", r.time, r.pid);
                }
                ViewData::Coredumps(rows)
            }
            Self::Sessions => {
                let rows: Vec<SessionRow> = run_json(
                    "loginctl",
                    &args(&["list-sessions", "--json=short", "--no-pager"]),
                    JsonShape::Array,
                )
                .await?;
                ViewData::Sessions(rows)
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

/// The D-Bus interface holding a unit type's own properties.
pub fn typed_interface(kind: &UnitKind) -> Option<String> {
    let name = match kind {
        UnitKind::Other(_) => return None,
        k => k.as_str(),
    };
    let mut chars = name.chars();
    let first = chars.next()?.to_ascii_uppercase();
    Some(format!(
        "org.freedesktop.systemd1.{first}{}",
        chars.as_str()
    ))
}

/// Everything for the detail view. If the object is gone (the unit was
/// garbage-collected), load it again and retry once.
async fn detail(backend: &Backend, name: &str, path: &OwnedObjectPath) -> Result<UnitDetail> {
    let unit = match properties(backend, path, "org.freedesktop.systemd1.Unit").await {
        Ok(p) => p,
        Err(_) => {
            let fresh = backend.manager.load_unit(name).await.context("LoadUnit")?;
            return Box::pin(detail(backend, name, &fresh)).await;
        }
    };
    let kind = UnitKind::of(name);
    let typed = match typed_interface(&kind) {
        Some(iface) => properties(backend, path, &iface).await.unwrap_or_default(),
        None => HashMap::new(),
    };
    let processes = backend
        .manager
        .get_unit_processes(name)
        .await
        .unwrap_or_default();

    let mut paths = Vec::new();
    if let Some(fragment) = prop_str(&unit, "FragmentPath").filter(|p| !p.is_empty()) {
        paths.push(fragment);
    }
    if let Some(v) = unit.get("DropInPaths")
        && let Ok(drop_ins) = Vec::<String>::try_from(v.clone())
    {
        paths.extend(drop_ins);
    }
    let mut files = Vec::with_capacity(paths.len());
    for p in paths {
        let contents = tokio::fs::read_to_string(&p)
            .await
            .map_err(|e| e.to_string());
        files.push((p, contents));
    }

    Ok(UnitDetail {
        name: name.to_owned(),
        path: path.clone(),
        unit,
        typed,
        processes,
        files,
        fetched_at: Instant::now(),
    })
}
