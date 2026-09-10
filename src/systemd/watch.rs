//! Live updates: Manager signals plus a debounced PropertiesChanged stream
//! over every unit object.

use std::{collections::HashSet, sync::Arc, time::Duration};

use eyre::{Context, Result};
use futures_util::StreamExt;
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tracing::{debug, warn};
use zbus::{
    MatchRule, MessageStream,
    message::Type,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::Backend;
use crate::event::Event;

const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub enum SystemdSignal {
    UnitNew(String),
    UnitRemoved(String),
    JobNew {
        unit: String,
    },
    JobRemoved {
        job: OwnedObjectPath,
        unit: String,
        result: String,
    },
    Reloading(bool),
    /// Units whose properties changed since the last flush.
    UnitsDirty(Vec<OwnedObjectPath>),
}

/// Subscribe and start the listener tasks. Must run before any action is
/// issued so that no `JobRemoved` can be missed.
pub async fn start(backend: Arc<Backend>, tx: Sender<Event>) -> Result<Vec<JoinHandle<()>>> {
    backend
        .manager
        .subscribe()
        .await
        .context("Manager.Subscribe")?;
    let m = &backend.manager;
    let mut unit_new = m.receive_unit_new().await.context("UnitNew stream")?;
    let mut unit_removed = m
        .receive_unit_removed()
        .await
        .context("UnitRemoved stream")?;
    let mut job_new = m.receive_job_new().await.context("JobNew stream")?;
    let mut job_removed = m.receive_job_removed().await.context("JobRemoved stream")?;
    let mut reloading = m.receive_reloading().await.context("Reloading stream")?;

    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path_namespace("/org/freedesktop/systemd1/unit")?
        .build();
    let mut props = MessageStream::for_match_rule(rule, &backend.conn, Some(4096))
        .await
        .context("PropertiesChanged stream")?;

    let mut handles = Vec::new();
    macro_rules! forward {
        ($stream:ident, |$args:ident| $sig:expr) => {{
            let tx = tx.clone();
            handles.push(tokio::spawn(async move {
                while let Some(msg) = $stream.next().await {
                    match msg.args() {
                        Ok($args) => {
                            if tx.send(Event::Signal($sig)).await.is_err() {
                                break;
                            }
                        }
                        Err(err) => warn!(%err, "bad signal"),
                    }
                }
            }));
        }};
    }
    forward!(unit_new, |a| SystemdSignal::UnitNew(a.id));
    forward!(unit_removed, |a| SystemdSignal::UnitRemoved(a.id));
    forward!(job_new, |a| SystemdSignal::JobNew { unit: a.unit });
    forward!(job_removed, |a| SystemdSignal::JobRemoved {
        job: a.job,
        unit: a.unit,
        result: a.result
    });
    forward!(reloading, |a| SystemdSignal::Reloading(a.active));

    let tx = tx.clone();
    handles.push(tokio::spawn(async move {
        let mut dirty: HashSet<OwnedObjectPath> = HashSet::new();
        let mut flush = tokio::time::interval(DEBOUNCE);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                msg = props.next() => {
                    let Some(Ok(msg)) = msg else { break };
                    let header = msg.header();
                    let Some(path) = header.path() else { continue };
                    // Only the systemd interfaces matter; skip other announcements.
                    let iface: Result<(String, std::collections::HashMap<String, OwnedValue>, Vec<String>), _> =
                        msg.body().deserialize();
                    match iface {
                        Ok((name, _, _)) if name.starts_with("org.freedesktop.systemd1.") => {
                            dirty.insert(OwnedObjectPath::from(path.to_owned()));
                        }
                        Ok(_) => {}
                        Err(err) => debug!(%err, "PropertiesChanged body"),
                    }
                }
                _ = flush.tick() => {
                    if dirty.is_empty() {
                        continue;
                    }
                    let paths: Vec<_> = dirty.drain().collect();
                    if tx.send(Event::Signal(SystemdSignal::UnitsDirty(paths))).await.is_err() {
                        break;
                    }
                }
            }
        }
    }));
    Ok(handles)
}
