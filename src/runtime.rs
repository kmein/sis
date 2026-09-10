//! The tokio side: owns the terminal, the event channel and the backend, and
//! executes the effects the [`App`] asks for.

use std::{collections::HashMap, sync::Arc};

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use eyre::Result;
use ratatui::DefaultTerminal;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tracing::{debug, error};

use crate::{
    app::App,
    event::{Effect, Event, Exec, Status, spawn_terminal_events, spawn_ticker},
    systemd::{
        Backend, Scope,
        actions::{self, Outcome},
        fetch,
        journal::{self, JournalHandle, JournalId},
        subprocess, watch,
    },
    ui,
};

pub struct Runtime {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    backend: Arc<Backend>,
    app: App,
    watchers: Vec<JoinHandle<()>>,
    journals: HashMap<JournalId, JournalHandle>,
    term_events: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn new(backend: Backend, initial_view: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        let app = App::new(backend.scope, initial_view);
        Self {
            tx,
            rx,
            backend: Arc::new(backend),
            app,
            watchers: Vec::new(),
            journals: HashMap::new(),
            term_events: None,
        }
    }

    /// (Re)start the signal listeners for the current backend.
    async fn watch(&mut self) {
        for handle in self.watchers.drain(..) {
            handle.abort();
        }
        match watch::start(Arc::clone(&self.backend), self.tx.clone()).await {
            Ok(handles) => self.watchers = handles,
            Err(err) => {
                error!(%err, "cannot subscribe to systemd signals");
                let _ = self
                    .tx
                    .send(Event::Error(format!("no live updates: {err:#}")))
                    .await;
            }
        }
    }

    pub async fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.term_events = Some(spawn_terminal_events(self.tx.clone()));
        spawn_ticker(self.tx.clone(), std::time::Duration::from_millis(500));
        self.watch().await;
        let effects = self.app.start();
        self.execute(effects, terminal).await;
        terminal.draw(|f| ui::draw(f, &mut self.app))?;

        while let Some(event) = self.rx.recv().await {
            let mut effects = self.handle(event).await;
            // Drain a burst (key repeat, signal storm) before drawing once.
            while let Ok(event) = self.rx.try_recv() {
                effects.extend(self.handle(event).await);
            }
            self.execute(effects, terminal).await;
            if self.app.should_quit {
                break;
            }
            terminal.draw(|f| ui::draw(f, &mut self.app))?;
        }
        Ok(())
    }

    /// Runtime-level bookkeeping before the app sees an event.
    async fn handle(&mut self, event: Event) -> Vec<Effect> {
        if let Event::BackendReady(backend) = &event {
            self.backend = Arc::clone(backend);
            self.watch().await;
        }
        self.app.update(event)
    }

    async fn execute(&mut self, effects: Vec<Effect>, terminal: &mut DefaultTerminal) {
        for effect in effects {
            match effect {
                Effect::Fetch(kind) => {
                    let backend = Arc::clone(&self.backend);
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let event = match kind.run(backend).await {
                            Ok(data) => Event::Data(data),
                            Err(err) => Event::Error(format!("{err:#}")),
                        };
                        let _ = tx.send(event).await;
                    });
                }
                Effect::Enrich(targets) => {
                    let backend = Arc::clone(&self.backend);
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let event = match fetch::enrich(backend, targets).await {
                            Ok(data) => Event::Data(data),
                            Err(err) => Event::Error(format!("{err:#}")),
                        };
                        let _ = tx.send(event).await;
                    });
                }
                Effect::Perform { action, unit } => {
                    let backend = Arc::clone(&self.backend);
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let event = match actions::perform(&backend, action, &unit).await {
                            Ok(Outcome::Job(job)) => Event::ActionStarted { action, unit, job },
                            Ok(Outcome::Done(msg)) => Event::ActionDone {
                                action,
                                unit,
                                outcome: Ok(msg),
                            },
                            Err(err) => Event::ActionDone {
                                action,
                                unit,
                                outcome: Err(err),
                            },
                        };
                        let _ = tx.send(event).await;
                    });
                }
                Effect::OpenJournal { id, spec } => {
                    match journal::spawn(id, &spec, self.tx.clone()) {
                        Ok(handle) => {
                            self.journals.insert(id, handle);
                        }
                        Err(err) => {
                            let tx = self.tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(Event::Error(format!("{err:#}"))).await;
                            });
                        }
                    }
                }
                Effect::CloseJournal(id) => {
                    if let Some(handle) = self.journals.remove(&id) {
                        handle.stop();
                    }
                }
                Effect::Exec(exec) => {
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let output = subprocess::run(&exec.program, &exec.args).await;
                        let _ = tx.send(Event::Exec { exec, output }).await;
                    });
                }
                Effect::SwitchScope(scope) => self.switch_scope(scope),
                Effect::Interactive(exec) => self.interactive(exec, terminal).await,
                Effect::Push(_) | Effect::Pop | Effect::Status(_) | Effect::Confirm { .. } => {
                    error!("UI effect reached the runtime; the app should have consumed it");
                }
            }
        }
    }

    /// Hand the terminal to a child process (a shell in a unit's namespace, a
    /// debugger), then take it back.
    async fn interactive(&mut self, exec: Exec, terminal: &mut DefaultTerminal) {
        if let Some(handle) = self.term_events.take() {
            // Dropping the crossterm stream stops it reading the tty.
            handle.abort();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        ratatui::restore();
        println!("sis: {}", exec.describe);
        let status = tokio::process::Command::new(&exec.program)
            .args(&exec.args)
            .status()
            .await;
        let outcome = match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("exited with {s}")),
            Err(err) => Err(format!("cannot run {}: {err}", exec.program)),
        };
        if let Err(msg) = &outcome {
            println!("sis: {}: {msg}. Press Enter to return.", exec.describe);
            let _ = tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)
            })
            .await;
        }
        let _ = enable_raw_mode();
        let _ = execute!(std::io::stdout(), EnterAlternateScreen);
        let _ = terminal.clear();
        self.term_events = Some(spawn_terminal_events(self.tx.clone()));
        let status = match outcome {
            Ok(()) => Status::ok(format!("{}: done", exec.describe)),
            Err(msg) => Status::error(format!("{}: {msg}", exec.describe)),
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(Event::Flash(status)).await;
        });
    }

    fn switch_scope(&mut self, scope: Scope) {
        let tx = self.tx.clone();
        let current = Arc::clone(&self.backend);
        tokio::spawn(async move {
            let event = match Backend::connect(scope).await {
                Ok(backend) => Event::BackendReady(Arc::new(backend)),
                Err(err) => {
                    debug!(%err, "scope switch failed");
                    let _ = tx
                        .send(Event::Error(format!(
                            "cannot reach the {scope} manager: {err:#}"
                        )))
                        .await;
                    Event::BackendReady(current)
                }
            };
            let _ = tx.send(event).await;
        });
    }
}
