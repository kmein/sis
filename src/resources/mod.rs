//! The view abstraction: a [`Resource`] describes a table of rows, a [`View`]
//! is something on the view stack that handles keys and draws itself.

pub mod journal;
pub mod text;
pub mod unit_detail;
pub mod units;

use std::{
    cmp::Ordering,
    time::{Duration, Instant},
};

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

use crate::systemd::journal::{JournalId, JournalItem};
use crate::{
    event::{Effect, Status},
    keys::{self, Action, Binding},
    store::{DataKind, Store},
    systemd::{
        Scope, actions::UnitAction, fetch::FetchKind, types::UnitKind, watch::SystemdSignal,
    },
    ui::theme::Theme,
};

/// Everything a view may read or request while handling an event.
pub struct Ctx<'a> {
    pub store: &'a Store,
    pub settings: &'a Settings,
    pub scope: Scope,
    pub now: Instant,
    pub theme: &'a Theme,
    effects: Vec<Effect>,
}

impl<'a> Ctx<'a> {
    pub fn new(store: &'a Store, settings: &'a Settings, scope: Scope, theme: &'a Theme) -> Self {
        Self {
            store,
            settings,
            scope,
            now: Instant::now(),
            theme,
            effects: Vec::new(),
        }
    }

    pub fn effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    pub fn fetch(&mut self, kind: FetchKind) {
        self.effect(Effect::Fetch(kind));
    }

    pub fn push(&mut self, view: Box<dyn View>) {
        self.effect(Effect::Push(view));
    }

    pub fn pop(&mut self) {
        self.effect(Effect::Pop);
    }

    pub fn status(&mut self, status: Status) {
        self.effect(Effect::Status(status));
    }

    /// Run a unit action, asking first if `confirm` is set.
    pub fn perform(&mut self, action: UnitAction, unit: &str, confirm: bool) {
        let effect = Effect::Perform {
            action,
            unit: unit.to_owned(),
        };
        if confirm {
            self.effect(Effect::Confirm {
                text: format!("{action} {unit}?"),
                effect: Box::new(effect),
            });
        } else {
            self.effect(effect);
        }
    }

    pub fn finish(self) -> Vec<Effect> {
        self.effects
    }
}

/// User-toggled settings that affect how rows are shown.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Show inactive units too (`ctrl-a`).
    pub show_all: bool,
    /// Restrict the units view to one unit type (`1`..`5`, `0` for all).
    pub kind: Option<UnitKind>,
}

impl Settings {
    /// The unit type a hot-key selects, if the action is one.
    pub fn kind_for(action: Action) -> Option<Option<UnitKind>> {
        Some(match action {
            Action::KindService => Some(UnitKind::Service),
            Action::KindTimer => Some(UnitKind::Timer),
            Action::KindSocket => Some(UnitKind::Socket),
            Action::KindTarget => Some(UnitKind::Target),
            Action::KindMount => Some(UnitKind::Mount),
            Action::KindAll => None,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub title: &'static str,
    pub width: Constraint,
    /// Right-align numbers.
    pub right: bool,
}

impl Column {
    pub const fn new(title: &'static str, width: Constraint) -> Self {
        Self {
            title,
            width,
            right: false,
        }
    }

    pub const fn right(self) -> Self {
        Self {
            right: true,
            ..self
        }
    }
}

/// A comparable projection of one cell, for sorting.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum SortKey {
    Str(String),
    Num(i128),
    /// Unset values sort last regardless of direction.
    None,
}

impl SortKey {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::None, Self::None) => Ordering::Equal,
            (Self::None, _) => Ordering::Greater,
            (_, Self::None) => Ordering::Less,
            (a, b) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        }
    }
}

/// Do not refetch more often than this on signal bursts.
const SIGNAL_THROTTLE: Duration = Duration::from_millis(250);
/// Do not ask for viewport enrichment more often than this.
const ENRICH_EVERY: Duration = Duration::from_secs(1);

pub enum Handled {
    Yes,
    No,
}

/// A tabular resource: what to fetch, how to turn the store into rows, how to
/// draw a row, and what keys do on a row.
pub trait Resource: 'static {
    type Row: Clone + Send + Sync + 'static;

    /// The `:name` that opens this view.
    const NAME: &'static str;
    const ALIASES: &'static [&'static str] = &[];

    fn columns() -> &'static [Column];

    /// Fetches this view needs, each with its refresh interval.
    fn fetches() -> &'static [(FetchKind, Duration)];

    /// Does a dataset arriving change our rows?
    fn affected_by(kind: DataKind) -> bool;

    fn rows(store: &Store, settings: &Settings) -> Vec<Self::Row>;

    /// Stable identity, used to keep the selection across refreshes.
    fn key(row: &Self::Row) -> &str;

    fn cells(row: &Self::Row, theme: &Theme) -> Vec<Cell<'static>>;

    fn sort_key(row: &Self::Row, col: usize) -> SortKey;

    /// Which column a sort action refers to, if any.
    fn sort_column(action: Action) -> Option<usize>;

    /// Default sort column.
    fn default_sort() -> usize {
        0
    }

    /// Plain-text search space for `/`.
    fn matches(row: &Self::Row, needle: &str) -> bool;

    fn bindings() -> &'static [Binding];

    fn on_action(row: &Self::Row, action: Action, ctx: &mut Ctx<'_>);

    /// Called with the rows currently on screen; request lazy data for them.
    fn enrich(_rows: &[&Self::Row], _ctx: &mut Ctx<'_>) {}

    /// Should this signal trigger a refetch of our data?
    fn interested(_signal: &SystemdSignal) -> bool {
        false
    }
}

/// Something on the view stack.
pub trait View: Send {
    /// Breadcrumb component, e.g. `units`.
    fn title(&self) -> String;

    /// Context-specific bindings for the header and the help overlay.
    fn bindings(&self) -> &'static [Binding];

    fn on_enter(&mut self, ctx: &mut Ctx<'_>);

    fn on_data(&mut self, kind: DataKind, ctx: &mut Ctx<'_>);

    fn on_tick(&mut self, ctx: &mut Ctx<'_>);

    fn on_signal(&mut self, _signal: &SystemdSignal, _ctx: &mut Ctx<'_>) {}

    fn on_journal(&mut self, _id: JournalId, _item: JournalItem) {}

    fn on_key(&mut self, key: &KeyEvent, ctx: &mut Ctx<'_>) -> Handled;

    /// A global action that no binding of the view claimed.
    fn on_global(&mut self, action: Action, ctx: &mut Ctx<'_>) -> Handled;

    fn set_filter(&mut self, needle: &str);

    fn filter(&self) -> &str;

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, ctx: &Ctx<'_>);

    fn on_close(&mut self, _ctx: &mut Ctx<'_>) {}
}

/// The generic table view over a [`Resource`].
pub struct TableView<R: Resource> {
    rows: Vec<R::Row>,
    /// Indices into `rows` after filtering and sorting.
    visible: Vec<usize>,
    selected: usize,
    selected_key: Option<String>,
    offset: usize,
    filter: String,
    sort: (usize, bool),
    last_fetch: Vec<Option<Instant>>,
    /// A signal asked for a refetch while one was too recent.
    refetch_pending: bool,
    last_enrich: Option<Instant>,
    last_height: usize,
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Resource> Default for TableView<R> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            selected_key: None,
            offset: 0,
            filter: String::new(),
            sort: (R::default_sort(), false),
            last_fetch: vec![None; R::fetches().len()],
            refetch_pending: false,
            last_enrich: None,
            last_height: 1,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<R: Resource> TableView<R> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn boxed() -> Box<dyn View> {
        Box::new(Self::new())
    }

    pub fn selected_row(&self) -> Option<&R::Row> {
        self.visible.get(self.selected).map(|&i| &self.rows[i])
    }

    fn rebuild(&mut self, ctx: &Ctx<'_>) {
        self.rows = R::rows(ctx.store, ctx.settings);
        self.recompute();
    }

    fn recompute(&mut self) {
        let needle = self.filter.to_lowercase();
        self.visible = (0..self.rows.len())
            .filter(|&i| needle.is_empty() || R::matches(&self.rows[i], &needle))
            .collect();
        let (col, desc) = self.sort;
        let rows = &self.rows;
        self.visible.sort_by(|&a, &b| {
            let ord = R::sort_key(&rows[a], col).cmp(&R::sort_key(&rows[b], col));
            let ord = if desc { ord.reverse() } else { ord };
            ord.then_with(|| R::key(&rows[a]).cmp(R::key(&rows[b])))
        });
        // Keep the cursor on the same row if it is still there.
        if let Some(key) = &self.selected_key
            && let Some(pos) = self
                .visible
                .iter()
                .position(|&i| R::key(&self.rows[i]) == key)
        {
            self.selected = pos;
        }
        self.clamp();
    }

    fn clamp(&mut self) {
        if self.visible.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.visible.len() - 1);
        }
        self.selected_key = self.selected_row().map(|r| R::key(r).to_owned());
        let height = self.last_height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    fn move_by(&mut self, delta: isize) {
        let len = self.visible.len() as isize;
        self.selected = (self.selected as isize + delta).clamp(0, (len - 1).max(0)) as usize;
        self.clamp();
    }

    fn enrich_viewport(&mut self, ctx: &mut Ctx<'_>) {
        self.last_enrich = Some(ctx.now);
        let end = (self.offset + self.last_height).min(self.visible.len());
        let rows: Vec<&R::Row> = self.visible[self.offset.min(end)..end]
            .iter()
            .map(|&i| &self.rows[i])
            .collect();
        R::enrich(&rows, ctx);
    }

    fn set_sort(&mut self, col: usize) {
        self.sort = if self.sort.0 == col {
            (col, !self.sort.1)
        } else {
            (col, false)
        };
        self.recompute();
    }

    fn sort_marker(&self, col: usize) -> &'static str {
        match self.sort {
            (c, false) if c == col => "↑",
            (c, true) if c == col => "↓",
            _ => "",
        }
    }
}

impl<R: Resource> View for TableView<R> {
    fn title(&self) -> String {
        R::NAME.to_owned()
    }

    fn bindings(&self) -> &'static [Binding] {
        R::bindings()
    }

    fn on_enter(&mut self, ctx: &mut Ctx<'_>) {
        for (i, (kind, _)) in R::fetches().iter().enumerate() {
            ctx.fetch(kind.clone());
            self.last_fetch[i] = Some(ctx.now);
        }
        self.rebuild(ctx);
    }

    fn on_data(&mut self, kind: DataKind, ctx: &mut Ctx<'_>) {
        if R::affected_by(kind) {
            self.rebuild(ctx);
            self.enrich_viewport(ctx);
        }
    }

    fn on_tick(&mut self, ctx: &mut Ctx<'_>) {
        for (i, (kind, every)) in R::fetches().iter().enumerate() {
            let due = self.last_fetch[i].is_none_or(|t| ctx.now.duration_since(t) >= *every);
            if due || (i == 0 && self.refetch_pending) {
                ctx.fetch(kind.clone());
                self.last_fetch[i] = Some(ctx.now);
                if i == 0 {
                    self.refetch_pending = false;
                }
            }
        }
        if self
            .last_enrich
            .is_none_or(|t| ctx.now.duration_since(t) >= ENRICH_EVERY)
        {
            self.enrich_viewport(ctx);
        }
    }

    fn on_signal(&mut self, signal: &SystemdSignal, ctx: &mut Ctx<'_>) {
        if !R::interested(signal) {
            return;
        }
        let Some((kind, _)) = R::fetches().first() else {
            return;
        };
        let recent =
            self.last_fetch[0].is_some_and(|t| ctx.now.duration_since(t) < SIGNAL_THROTTLE);
        if recent {
            self.refetch_pending = true;
        } else {
            ctx.fetch(kind.clone());
            self.last_fetch[0] = Some(ctx.now);
        }
    }

    fn on_key(&mut self, key: &KeyEvent, ctx: &mut Ctx<'_>) -> Handled {
        let Some(binding) = keys::lookup(R::bindings(), key) else {
            return Handled::No;
        };
        let Some(row) = self.selected_row().cloned() else {
            return Handled::Yes;
        };
        R::on_action(&row, binding.action, ctx);
        Handled::Yes
    }

    fn on_global(&mut self, action: Action, ctx: &mut Ctx<'_>) -> Handled {
        let page = self.last_height.max(1) as isize;
        match action {
            Action::Up => self.move_by(-1),
            Action::Down => self.move_by(1),
            Action::PageUp => self.move_by(-page),
            Action::PageDown => self.move_by(page),
            Action::Top => self.move_by(isize::MIN / 2),
            Action::Bottom => self.move_by(isize::MAX / 2),
            Action::Refresh => {
                for (i, (kind, _)) in R::fetches().iter().enumerate() {
                    ctx.fetch(kind.clone());
                    self.last_fetch[i] = Some(ctx.now);
                }
            }
            Action::ToggleAll | Action::SettingsChanged => self.rebuild(ctx),
            other => match R::sort_column(other) {
                Some(col) => self.set_sort(col),
                None => return Handled::No,
            },
        }
        Handled::Yes
    }

    fn set_filter(&mut self, needle: &str) {
        self.filter = needle.to_owned();
        self.recompute();
    }

    fn filter(&self) -> &str {
        &self.filter
    }

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, ctx: &Ctx<'_>) {
        let theme = ctx.theme;
        let title = if self.filter.is_empty() {
            format!(" {}({}) ", R::NAME, self.visible.len())
        } else {
            format!(" {}(/{})[{}] ", R::NAME, self.filter, self.visible.len())
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border)
            .title(title);
        let inner = block.inner(area);
        // One line for the column headers.
        self.last_height = inner.height.saturating_sub(1).max(1) as usize;
        self.clamp();

        let columns = R::columns();
        let header = Row::new(columns.iter().enumerate().map(|(i, c)| {
            Cell::from(format!("{}{}", c.title, self.sort_marker(i))).style(theme.header)
        }));
        let end = (self.offset + self.last_height).min(self.visible.len());
        let rows = self.visible[self.offset..end]
            .iter()
            .map(|&i| Row::new(R::cells(&self.rows[i], theme)));
        let widths: Vec<Constraint> = columns.iter().map(|c| c.width).collect();
        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(theme.selected)
            .column_spacing(1)
            .style(Style::default());
        let mut state = TableState::default().with_selected(Some(self.selected - self.offset));
        f.render_stateful_widget(table, area, &mut state);
    }
}

/// The `:command` registry.
struct Entry {
    name: &'static str,
    aliases: &'static [&'static str],
    open: fn() -> Box<dyn View>,
}

const REGISTRY: &[Entry] = &[Entry {
    name: units::UnitsResource::NAME,
    aliases: units::UnitsResource::ALIASES,
    open: TableView::<units::UnitsResource>::boxed,
}];

/// Open the view registered under `name` or one of its aliases.
pub fn lookup(name: &str) -> Option<Box<dyn View>> {
    REGISTRY
        .iter()
        .find(|e| e.name == name || e.aliases.contains(&name))
        .map(|e| (e.open)())
}

/// Canonical view names, for help and completion.
pub fn names() -> Vec<&'static str> {
    REGISTRY.iter().map(|e| e.name).collect()
}

/// Names starting with `prefix`, for tab completion.
pub fn complete(prefix: &str) -> Vec<&'static str> {
    REGISTRY
        .iter()
        .map(|e| e.name)
        .filter(|n| n.starts_with(prefix))
        .collect()
}
