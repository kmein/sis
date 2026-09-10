//! `:timers` — `systemctl list-timers`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{
    Column, Ctx, Resource, Settings, SortKey,
    journal::JournalView,
    unit_detail::{Tab, UnitDetailView},
    units,
};
use crate::{
    keys::{Action, Binding, Key},
    store::{DataKind, Store, TimerRow},
    systemd::{fetch::FetchKind, journal::JournalSpec, watch::SystemdSignal},
    ui::{format, theme::Theme},
};

pub struct TimersResource;

const COLUMNS: &[Column] = &[
    Column::new("UNIT", Constraint::Min(24)),
    Column::new("NEXT", Constraint::Length(24)),
    Column::new("LEFT", Constraint::Length(8)).right(),
    Column::new("LAST", Constraint::Length(24)),
    Column::new("PASSED", Constraint::Length(8)).right(),
    Column::new("ACTIVATES", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Describe"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs of activated"),
    Binding::new(Key::ch('s'), Action::Start, "Start"),
    Binding::new(Key::ch('x'), Action::Stop, "Stop").confirm(),
    Binding::new(Key::ch('r'), Action::Restart, "Restart").confirm(),
    Binding::new(Key::ch('e'), Action::Enable, "Enable"),
    Binding::new(Key::ch('d'), Action::Disable, "Disable").confirm(),
    Binding::new(Key::ch('c'), Action::Cat, "Cat unit file"),
];

impl Resource for TimersResource {
    type Row = TimerRow;

    const ALIASES: &'static [&'static str] = &["timer", "t"];
    const NAME: &'static str = "timers";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Timers, Duration::from_secs(5))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Timers
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<TimerRow> {
        store.timers.clone()
    }

    fn key(row: &TimerRow) -> &str {
        &row.unit
    }

    fn cells(r: &TimerRow, theme: &Theme) -> Vec<Cell<'static>> {
        // systemctl's `left`/`passed` are not relative on systemd 260; derive them.
        let left = r
            .next
            .filter(|n| *n != 0)
            .map(format::until)
            .unwrap_or_default();
        let passed = r
            .last
            .filter(|n| *n != 0)
            .map(format::age)
            .unwrap_or_default();
        vec![
            Cell::from(r.unit.clone()).style(theme.name),
            Cell::from(r.next.map(format::timestamp).unwrap_or_default()),
            Cell::from(left).style(theme.ok),
            Cell::from(r.last.map(format::timestamp).unwrap_or_default()).style(theme.dim),
            Cell::from(passed).style(theme.dim),
            Cell::from(r.activates.clone().unwrap_or_default()),
        ]
    }

    fn sort_key(r: &TimerRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.unit.clone()),
            1 | 2 => r.next.map_or(SortKey::None, |n| SortKey::Num(n as i128)),
            3 | 4 => r.last.map_or(SortKey::None, |n| SortKey::Num(n as i128)),
            _ => SortKey::Str(r.activates.clone().unwrap_or_default()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortActive => Some(1),
            Action::SortLoad => Some(3),
            _ => None,
        }
    }

    fn default_sort() -> usize {
        1
    }

    fn matches(r: &TimerRow, needle: &str) -> bool {
        r.unit.to_lowercase().contains(needle)
            || r.activates
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &TimerRow, action: Action, ctx: &mut Ctx<'_>) {
        let confirm = BINDINGS.iter().any(|b| b.action == action && b.confirm);
        let path = ctx.store.unit_path(&row.unit);
        match action {
            Action::Select => ctx.push(UnitDetailView::boxed(&row.unit, path, Tab::Status)),
            Action::Cat => ctx.push(UnitDetailView::boxed(&row.unit, path, Tab::File)),
            Action::Logs => {
                let unit = row.activates.clone().unwrap_or_else(|| row.unit.clone());
                ctx.push(JournalView::boxed(JournalSpec::unit(ctx.scope, &unit)));
            }
            other => units::perform(&row.unit, other, confirm, ctx),
        }
    }

    fn interested(signal: &SystemdSignal) -> bool {
        matches!(
            signal,
            SystemdSignal::JobRemoved { .. }
                | SystemdSignal::UnitNew(_)
                | SystemdSignal::UnitRemoved(_)
        )
    }
}
