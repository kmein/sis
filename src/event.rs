//! Events flowing into [`crate::app::App`] and effects flowing out of it.

use std::{sync::Arc, time::Duration};

use crossterm::event::{Event as TermEvent, EventStream, KeyEvent, KeyEventKind};
use futures_util::StreamExt;
use tokio::sync::mpsc::Sender;
use tracing::warn;

use zbus::zvariant::OwnedObjectPath;

use crate::{
    resources::View,
    store::ViewData,
    systemd::{Backend, Scope, actions::UnitAction, fetch::FetchKind, watch::SystemdSignal},
};

/// Everything the application reacts to.
pub enum Event {
    Key(KeyEvent),
    Resize,
    Tick,
    /// A fetch finished successfully.
    Data(ViewData),
    /// A fetch or action failed.
    Error(String),
    /// The backend for a new scope is connected.
    BackendReady(Arc<Backend>),
    Signal(SystemdSignal),
    /// An action was accepted and produced a job to wait for.
    ActionStarted {
        action: UnitAction,
        unit: String,
        job: OwnedObjectPath,
    },
    /// An action finished without a job, or failed.
    ActionDone {
        action: UnitAction,
        unit: String,
        outcome: Result<String, String>,
    },
}

/// Everything the application asks the outside world to do. The UI-only
/// effects (`Push`, `Pop`, `Status`, `Confirm`) are consumed by
/// [`crate::app::App`] itself; the rest reach [`crate::runtime`].
pub enum Effect {
    Fetch(FetchKind),
    /// Fetch per-unit properties for these units (name, object path).
    Enrich(Vec<(String, OwnedObjectPath)>),
    Perform {
        action: UnitAction,
        unit: String,
    },
    SwitchScope(Scope),
    Quit,
    Push(Box<dyn View>),
    Pop,
    Status(Status),
    /// Ask before running the wrapped effect.
    Confirm {
        text: String,
        effect: Box<Effect>,
    },
}

/// A message for the status line.
#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub level: Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Error,
}

impl Status {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Info,
        }
    }

    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Ok,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Error,
        }
    }
}

/// Forward terminal key presses and resizes to the event channel.
pub fn spawn_terminal_events(tx: Sender<Event>) {
    tokio::spawn(async move {
        let mut stream = EventStream::new();
        while let Some(item) = stream.next().await {
            let event = match item {
                Ok(TermEvent::Key(key)) if key.kind == KeyEventKind::Press => Event::Key(key),
                Ok(TermEvent::Resize(..)) => Event::Resize,
                Ok(_) => continue,
                Err(err) => {
                    warn!(%err, "terminal event stream failed");
                    break;
                }
            };
            if tx.send(event).await.is_err() {
                break;
            }
        }
    });
}

/// Send [`Event::Tick`] at a fixed interval.
pub fn spawn_ticker(tx: Sender<Event>, every: Duration) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(every);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if tx.send(Event::Tick).await.is_err() {
                break;
            }
        }
    });
}
