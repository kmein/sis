//! The tokio side: owns the terminal, the event channel and the backend, and
//! executes the effects the [`App`] asks for.

use std::sync::Arc;

use eyre::Result;
use ratatui::DefaultTerminal;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tracing::{debug, error};

use crate::{
    app::App,
    event::{Effect, Event, spawn_terminal_events, spawn_ticker},
    systemd::{
        Backend, Scope,
        actions::{self, Outcome},
        fetch, watch,
    },
    ui,
};

pub struct Runtime {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    backend: Arc<Backend>,
    app: App,
    watchers: Vec<JoinHandle<()>>,
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
        spawn_terminal_events(self.tx.clone());
        spawn_ticker(self.tx.clone(), std::time::Duration::from_millis(500));
        self.watch().await;
        let effects = self.app.start();
        self.execute(effects);
        terminal.draw(|f| ui::draw(f, &mut self.app))?;

        while let Some(event) = self.rx.recv().await {
            let mut effects = self.handle(event).await;
            // Drain a burst (key repeat, signal storm) before drawing once.
            while let Ok(event) = self.rx.try_recv() {
                effects.extend(self.handle(event).await);
            }
            self.execute(effects);
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

    fn execute(&mut self, effects: Vec<Effect>) {
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
                Effect::SwitchScope(scope) => self.switch_scope(scope),
                Effect::Quit => self.app.should_quit = true,
                Effect::Push(_) | Effect::Pop | Effect::Status(_) | Effect::Confirm { .. } => {
                    error!("UI effect reached the runtime; the app should have consumed it");
                }
            }
        }
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
