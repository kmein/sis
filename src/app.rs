//! The synchronous heart: state in, effects out. No I/O happens here.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};
use tracing::debug;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    event::{Effect, Event, Status},
    keys::{self, Action},
    resources::{self, Ctx, Handled, Settings, View},
    store::Store,
    systemd::{Backend, Scope, actions::UnitAction, fetch::FetchKind, watch::SystemdSignal},
    ui::theme::Theme,
};

const FLASH_FOR: Duration = Duration::from_secs(6);
const MANAGER_EVERY: Duration = Duration::from_secs(2);
/// Give up waiting for a job's result after this long.
const JOB_TIMEOUT: Duration = Duration::from_secs(120);

/// Correlates `ActionStarted` job paths with `JobRemoved` signals. The signal
/// can arrive before the method reply, hence the `recent` ring.
#[derive(Default)]
struct JobTracker {
    pending: HashMap<OwnedObjectPath, (UnitAction, String, Instant)>,
    recent: VecDeque<(OwnedObjectPath, String)>,
}

impl JobTracker {
    fn started(
        &mut self,
        job: OwnedObjectPath,
        action: UnitAction,
        unit: String,
        now: Instant,
    ) -> Option<Status> {
        if let Some(pos) = self.recent.iter().position(|(j, _)| *j == job) {
            let (_, result) = self.recent.remove(pos).expect("position exists");
            return Some(job_status(action, &unit, &result));
        }
        self.pending.insert(job, (action, unit, now));
        None
    }

    fn removed(&mut self, job: OwnedObjectPath, result: String) -> Option<Status> {
        match self.pending.remove(&job) {
            Some((action, unit, _)) => Some(job_status(action, &unit, &result)),
            None => {
                self.recent.push_back((job, result));
                if self.recent.len() > 64 {
                    self.recent.pop_front();
                }
                None
            }
        }
    }

    fn expire(&mut self, now: Instant) -> Vec<Status> {
        let mut out = Vec::new();
        self.pending.retain(|_, (action, unit, at)| {
            let keep = now.duration_since(*at) < JOB_TIMEOUT;
            if !keep {
                out.push(Status::info(format!("{action} {unit}: still running")));
            }
            keep
        });
        out
    }
}

fn job_status(action: UnitAction, unit: &str, result: &str) -> Status {
    match result {
        "done" => Status::ok(format!("{action} {unit}: done")),
        "skipped" => Status::info(format!("{action} {unit}: skipped (condition not met)")),
        other => Status::error(format!("{action} {unit}: {other} (press l for logs)")),
    }
}

pub enum Prompt {
    None,
    Command(String),
    Filter(String),
    Confirm {
        text: String,
        effect: Option<Effect>,
    },
}

pub struct App {
    pub scope: Scope,
    pub store: Store,
    pub settings: Settings,
    pub theme: Theme,
    pub prompt: Prompt,
    pub help: bool,
    pub should_quit: bool,
    pub connecting: bool,
    /// The manager is between `Reloading(true)` and `Reloading(false)`.
    pub reloading: bool,
    views: Vec<Box<dyn View>>,
    status: Option<(Status, Instant)>,
    last_manager: Option<Instant>,
    initial_view: String,
    jobs: JobTracker,
}

impl App {
    pub fn new(scope: Scope, initial_view: Option<String>) -> Self {
        Self {
            scope,
            store: Store::default(),
            settings: Settings::default(),
            theme: Theme::default(),
            prompt: Prompt::None,
            help: false,
            should_quit: false,
            connecting: false,
            reloading: false,
            views: Vec::new(),
            status: None,
            last_manager: None,
            initial_view: initial_view.unwrap_or_else(|| "units".to_owned()),
            jobs: JobTracker::default(),
        }
    }

    /// Open the root view; returns the fetches it needs.
    pub fn start(&mut self) -> Vec<Effect> {
        let view = match resources::lookup(&self.initial_view) {
            Some(view) => view,
            None => {
                self.flash(Status::error(format!(
                    "no such view: {}",
                    self.initial_view
                )));
                resources::lookup("units").expect("the units view exists")
            }
        };
        let mut effects = self.apply(vec![Effect::Push(view)]);
        effects.push(Effect::Fetch(FetchKind::Manager));
        self.last_manager = Some(Instant::now());
        effects
    }

    pub fn view(&self) -> &dyn View {
        self.views.last().expect("at least one view").as_ref()
    }

    pub fn breadcrumbs(&self) -> Vec<String> {
        self.views.iter().map(|v| v.title()).collect()
    }

    pub fn flash(&mut self, status: Status) {
        self.status = Some((status, Instant::now()));
    }

    /// The current status message, if it has not expired.
    pub fn flash_message(&self) -> Option<&Status> {
        self.status
            .as_ref()
            .filter(|(_, at)| at.elapsed() < FLASH_FOR)
            .map(|(s, _)| s)
    }

    pub fn render_view(&mut self, f: &mut Frame<'_>, area: Rect) {
        let ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
        if let Some(view) = self.views.last_mut() {
            view.render(f, area, &ctx);
        }
    }

    /// Handle one event; returns the effects the runtime has to execute.
    pub fn update(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Key(key) => self.on_key(key),
            Event::Resize => Vec::new(),
            Event::Tick => self.on_tick(),
            Event::Data(data) => {
                let kind = data.kind();
                self.store.apply(data);
                let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
                for view in &mut self.views {
                    view.on_data(kind, &mut ctx);
                }
                let effects = ctx.finish();
                self.apply(effects)
            }
            Event::Error(text) => {
                debug!(%text, "error event");
                self.flash(Status::error(text));
                Vec::new()
            }
            Event::BackendReady(backend) => self.on_backend(backend),
            Event::Signal(signal) => self.on_signal(signal),
            Event::ActionStarted { action, unit, job } => {
                if let Some(status) = self.jobs.started(job, action, unit, Instant::now()) {
                    self.flash(status);
                }
                Vec::new()
            }
            Event::ActionDone {
                action,
                unit,
                outcome,
            } => {
                self.flash(match outcome {
                    Ok(msg) => Status::ok(msg),
                    Err(err) => Status::error(format!("{action} {unit}: {err}")),
                });
                Vec::new()
            }
        }
    }

    fn on_signal(&mut self, signal: SystemdSignal) -> Vec<Effect> {
        let mut effects = Vec::new();
        match &signal {
            SystemdSignal::Reloading(active) => {
                self.reloading = *active;
                if !active {
                    effects.push(Effect::Fetch(FetchKind::Units));
                    effects.push(Effect::Fetch(FetchKind::UnitFiles));
                }
            }
            SystemdSignal::JobRemoved { job, result, .. } => {
                if let Some(status) = self.jobs.removed(job.clone(), result.clone()) {
                    self.flash(status);
                }
            }
            _ => {}
        }
        if !self.reloading {
            let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
            for view in &mut self.views {
                view.on_signal(&signal, &mut ctx);
            }
            effects.extend(ctx.finish());
        }
        self.apply(effects)
    }

    fn on_backend(&mut self, backend: Arc<Backend>) -> Vec<Effect> {
        self.connecting = false;
        self.reloading = false;
        self.jobs = JobTracker::default();
        self.scope = backend.scope;
        self.store.clear();
        let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
        for view in &mut self.views {
            view.on_close(&mut ctx);
        }
        let mut effects = ctx.finish();
        self.views.clear();
        effects.extend(self.start());
        self.flash(Status::ok(format!(
            "connected to the {} manager",
            self.scope
        )));
        effects
    }

    fn on_tick(&mut self) -> Vec<Effect> {
        let now = Instant::now();
        let mut effects = Vec::new();
        for status in self.jobs.expire(now) {
            self.flash(status);
        }
        if !self.connecting
            && self
                .last_manager
                .is_none_or(|t| now.duration_since(t) >= MANAGER_EVERY)
        {
            effects.push(Effect::Fetch(FetchKind::Manager));
            self.last_manager = Some(now);
        }
        if !self.connecting && !self.reloading {
            let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
            if let Some(view) = self.views.last_mut() {
                view.on_tick(&mut ctx);
            }
            effects.extend(ctx.finish());
        }
        self.apply(effects)
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.help {
            self.help = false;
            return Vec::new();
        }
        if !matches!(self.prompt, Prompt::None) {
            return self.on_prompt_key(key);
        }
        let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
        let view = self.views.last_mut().expect("at least one view");
        if let Handled::Yes = view.on_key(&key, &mut ctx) {
            let effects = ctx.finish();
            return self.apply(effects);
        }
        let Some(binding) = keys::lookup(keys::GLOBAL, &key) else {
            return Vec::new();
        };
        match binding.action {
            Action::Quit => {
                if key.code == KeyCode::Char('q') && self.views.len() > 1 {
                    return self.apply(vec![Effect::Pop]);
                }
                self.should_quit = true;
                Vec::new()
            }
            Action::Back => {
                if !view.filter().is_empty() {
                    view.set_filter("");
                    Vec::new()
                } else if self.views.len() > 1 {
                    self.apply(vec![Effect::Pop])
                } else {
                    Vec::new()
                }
            }
            Action::Help => {
                self.help = true;
                Vec::new()
            }
            Action::Command => {
                self.prompt = Prompt::Command(String::new());
                Vec::new()
            }
            Action::Filter => {
                self.prompt = Prompt::Filter(view.filter().to_owned());
                Vec::new()
            }
            Action::ToggleScope => self.switch_scope(self.scope.toggle()),
            Action::ToggleAll => {
                self.settings.show_all = !self.settings.show_all;
                self.settings_changed()
            }
            action if Settings::kind_for(action).is_some() => {
                self.settings.kind = Settings::kind_for(action).flatten();
                self.settings_changed()
            }
            action => {
                view.on_global(action, &mut ctx);
                let effects = ctx.finish();
                self.apply(effects)
            }
        }
    }

    /// Let every view rebuild its rows after a settings change.
    fn settings_changed(&mut self) -> Vec<Effect> {
        let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
        for view in &mut self.views {
            view.on_global(Action::SettingsChanged, &mut ctx);
        }
        let effects = ctx.finish();
        self.apply(effects)
    }

    fn on_prompt_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let prompt = std::mem::replace(&mut self.prompt, Prompt::None);
        match prompt {
            Prompt::None => Vec::new(),
            Prompt::Confirm { text, effect } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    let _ = text;
                    self.apply(effect.into_iter().collect())
                }
                _ => {
                    self.flash(Status::info("cancelled"));
                    Vec::new()
                }
            },
            Prompt::Command(mut text) => match key.code {
                KeyCode::Esc => Vec::new(),
                KeyCode::Enter => self.run_command(text.trim()),
                KeyCode::Tab => {
                    let matches = resources::complete(&text);
                    if let [only] = matches[..] {
                        text = only.to_owned();
                    } else if !matches.is_empty() {
                        self.flash(Status::info(matches.join("  ")));
                    }
                    self.prompt = Prompt::Command(text);
                    Vec::new()
                }
                _ => {
                    edit(&mut text, key);
                    self.prompt = Prompt::Command(text);
                    Vec::new()
                }
            },
            Prompt::Filter(mut text) => match key.code {
                KeyCode::Esc => {
                    self.views.last_mut().expect("a view").set_filter("");
                    Vec::new()
                }
                KeyCode::Enter => Vec::new(),
                _ => {
                    edit(&mut text, key);
                    self.views.last_mut().expect("a view").set_filter(&text);
                    self.prompt = Prompt::Filter(text);
                    Vec::new()
                }
            },
        }
    }

    fn run_command(&mut self, command: &str) -> Vec<Effect> {
        let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
        match name {
            "" => Vec::new(),
            "q" | "quit" => {
                self.should_quit = true;
                Vec::new()
            }
            "h" | "help" | "?" => {
                self.help = true;
                Vec::new()
            }
            "user" => self.switch_scope(Scope::User),
            "system" => self.switch_scope(Scope::System),
            "all" => {
                self.settings.show_all = !self.settings.show_all;
                self.settings_changed()
            }
            _ => match resources::lookup(name) {
                Some(view) => {
                    // A command replaces the stack: `:timers` is a place, not a drill-down.
                    let mut ctx = Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
                    for view in &mut self.views {
                        view.on_close(&mut ctx);
                    }
                    let mut effects = ctx.finish();
                    self.views.clear();
                    effects.push(Effect::Push(view));
                    let mut effects = self.apply(effects);
                    if !arg.is_empty() {
                        self.views.last_mut().expect("a view").set_filter(arg);
                    }
                    effects.retain(|e| !matches!(e, Effect::Quit));
                    effects
                }
                None => {
                    self.flash(Status::error(format!("unknown command: {name}")));
                    Vec::new()
                }
            },
        }
    }

    fn switch_scope(&mut self, scope: Scope) -> Vec<Effect> {
        if scope == self.scope || self.connecting {
            return Vec::new();
        }
        self.connecting = true;
        self.flash(Status::info(format!("connecting to the {scope} manager…")));
        vec![Effect::SwitchScope(scope)]
    }

    /// Consume the UI-side effects, hand the rest back for the runtime.
    fn apply(&mut self, effects: Vec<Effect>) -> Vec<Effect> {
        let mut queue = effects;
        let mut out = Vec::new();
        while !queue.is_empty() {
            let mut next = Vec::new();
            for effect in queue {
                match effect {
                    Effect::Push(mut view) => {
                        let mut ctx =
                            Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
                        view.on_enter(&mut ctx);
                        next.extend(ctx.finish());
                        self.views.push(view);
                    }
                    Effect::Pop => {
                        if self.views.len() > 1
                            && let Some(mut view) = self.views.pop()
                        {
                            let mut ctx =
                                Ctx::new(&self.store, &self.settings, self.scope, &self.theme);
                            view.on_close(&mut ctx);
                            next.extend(ctx.finish());
                        }
                    }
                    Effect::Status(status) => self.flash(status),
                    Effect::Quit => self.should_quit = true,
                    Effect::Confirm { text, effect } => {
                        self.prompt = Prompt::Confirm {
                            text,
                            effect: Some(*effect),
                        };
                    }
                    Effect::Perform { action, unit } => {
                        self.flash(Status::info(format!("{} {unit}…", action.progressive())));
                        out.push(Effect::Perform { action, unit });
                    }
                    other => out.push(other),
                }
            }
            queue = next;
        }
        out
    }
}

/// Minimal line editing for prompts.
fn edit(text: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Backspace => {
            text.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => text.clear(),
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let trimmed = text.trim_end();
            let cut = trimmed.rfind(' ').map_or(0, |i| i + 1);
            text.truncate(cut);
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
        _ => {}
    }
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Command(t) => write!(f, "Command({t:?})"),
            Self::Filter(t) => write!(f, "Filter({t:?})"),
            Self::Confirm { text, .. } => write!(f, "Confirm({text:?})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn start_fetches_units_and_manager() {
        let mut app = App::new(Scope::System, None);
        let effects = app.start();
        let kinds: Vec<_> = effects
            .iter()
            .filter_map(|e| match e {
                Effect::Fetch(k) => Some(k.clone()),
                _ => None,
            })
            .collect();
        assert!(kinds.contains(&FetchKind::Units));
        assert!(kinds.contains(&FetchKind::UnitFiles));
        assert!(kinds.contains(&FetchKind::Manager));
    }

    #[test]
    fn q_quits_at_root_and_colon_opens_prompt() {
        let mut app = App::new(Scope::System, None);
        app.start();
        app.update(key(':'));
        assert!(matches!(app.prompt, Prompt::Command(_)));
        app.update(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(matches!(app.prompt, Prompt::None));
        app.update(key('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn stop_asks_first_and_y_performs() {
        use crate::{store::ViewData, systemd::unit::Unit};
        let mut app = App::new(Scope::System, None);
        app.start();
        let listed = crate::systemd::types::ListedUnit(
            "foo.service".into(),
            "Foo".into(),
            "loaded".into(),
            "active".into(),
            "running".into(),
            String::new(),
            OwnedObjectPath::try_from("/org/freedesktop/systemd1/unit/foo_2eservice").unwrap(),
            0,
            String::new(),
            OwnedObjectPath::try_from("/").unwrap(),
        );
        app.update(Event::Data(ViewData::Units(vec![Unit::from_listed(
            listed,
        )])));
        assert!(app.update(key('x')).is_empty());
        assert!(matches!(app.prompt, Prompt::Confirm { .. }));
        let effects = app.update(key('y'));
        assert!(matches!(
            effects[..],
            [Effect::Perform {
                action: UnitAction::Stop,
                ..
            }]
        ));
        assert!(
            app.flash_message()
                .is_some_and(|s| s.text.starts_with("stopping foo.service"))
        );
        // `s` needs no confirmation.
        let effects = app.update(key('s'));
        assert!(matches!(
            effects[..],
            [Effect::Perform {
                action: UnitAction::Start,
                ..
            }]
        ));
    }

    #[test]
    fn job_result_is_reported_even_if_signal_beats_reply() {
        let mut app = App::new(Scope::System, None);
        app.start();
        let job = OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/42").unwrap();
        app.update(Event::Signal(SystemdSignal::JobRemoved {
            id: 42,
            job: job.clone(),
            unit: "foo.service".into(),
            result: "failed".into(),
        }));
        app.update(Event::ActionStarted {
            action: UnitAction::Restart,
            unit: "foo.service".into(),
            job,
        });
        assert!(
            app.flash_message()
                .is_some_and(|s| s.text.contains("restart foo.service: failed"))
        );
    }

    #[test]
    fn unknown_command_flashes_error() {
        let mut app = App::new(Scope::System, None);
        app.start();
        app.update(key(':'));
        for c in "nope".chars() {
            app.update(key(c));
        }
        app.update(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(
            app.flash_message()
                .is_some_and(|s| s.text.contains("unknown command"))
        );
    }
}
