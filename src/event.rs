//! Events flowing into [`crate::app::App`] and effects flowing out of it.

use std::{sync::Arc, time::Duration};

use crossterm::event::{Event as TermEvent, EventStream, KeyEvent, KeyEventKind};
use futures_util::StreamExt;
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tracing::warn;

use zbus::zvariant::OwnedObjectPath;

use crate::{
    resources::View,
    store::ViewData,
    systemd::{
        Backend, Scope,
        actions::UnitAction,
        fetch::FetchKind,
        journal::{JournalId, JournalItem, JournalSpec},
        watch::SystemdSignal,
    },
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
    /// A line (or the end) of a journal tail.
    Journal {
        id: JournalId,
        item: JournalItem,
    },
    /// A command finished.
    Exec {
        exec: Exec,
        output: Result<String, String>,
    },
    /// A status line message from the runtime.
    Flash(Status),
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
    OpenJournal {
        id: JournalId,
        spec: JournalSpec,
    },
    CloseJournal(JournalId),
    /// Leave the TUI, run this with the terminal, come back.
    Interactive(Exec),
    /// Run a command; show its output in a text view when `title` is set,
    /// otherwise report success or failure in the status line.
    Exec(Exec),
    SwitchScope(Scope),
    Push(Box<dyn View>),
    Pop,
    Status(Status),
    /// Ask before running the wrapped effect.
    Confirm {
        text: String,
        effect: Box<Effect>,
    },
}

/// A command to run outside the bus.
#[derive(Debug, Clone)]
pub struct Exec {
    pub program: String,
    pub args: Vec<String>,
    pub title: Option<String>,
    /// What to call it in the status line.
    pub describe: String,
    /// Deliver the output to the text view with this id instead of flashing.
    pub deliver: Option<u64>,
}

impl Exec {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            title: None,
            describe: format!("{program} {}", args.join(" ")),
            deliver: None,
        }
    }

    /// A shell snippet, described by `describe`.
    pub fn shell(script: &str, describe: &str) -> Self {
        Self {
            program: "sh".to_owned(),
            args: vec!["-c".to_owned(), script.to_owned()],
            title: None,
            describe: describe.to_owned(),
            deliver: None,
        }
    }

    pub fn show(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
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
pub fn spawn_terminal_events(tx: Sender<Event>) -> JoinHandle<()> {
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
    })
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
