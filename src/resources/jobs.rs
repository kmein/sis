//! `:jobs` — the manager's job queue.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{
    Column, Ctx, Resource, Settings, SortKey,
    journal::JournalView,
    unit_detail::{Tab, UnitDetailView},
};
use crate::{
    keys::{Action, Binding, Key},
    store::{DataKind, JobRow, Store},
    systemd::{actions::UnitAction, fetch::FetchKind, journal::JournalSpec, watch::SystemdSignal},
    ui::theme::Theme,
};

pub struct JobsResource;

const COLUMNS: &[Column] = &[
    Column::new("JOB", Constraint::Length(8)),
    Column::new("UNIT", Constraint::Min(30)),
    Column::new("TYPE", Constraint::Length(20)),
    Column::new("STATE", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Describe unit"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('x'), Action::Stop, "Cancel job").confirm(),
];

impl Resource for JobsResource {
    type Row = JobRow;

    const ALIASES: &'static [&'static str] = &["job", "j"];
    const NAME: &'static str = "jobs";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Jobs, Duration::from_secs(2))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Jobs
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<JobRow> {
        store.jobs.clone()
    }

    fn key(row: &JobRow) -> &str {
        &row.key
    }

    fn cells(r: &JobRow, theme: &Theme) -> Vec<Cell<'static>> {
        let state = if r.state == "running" {
            theme.ok
        } else {
            theme.warn
        };
        vec![
            Cell::from(r.id.to_string()).style(theme.dim),
            Cell::from(r.unit.clone()).style(theme.name),
            Cell::from(r.kind.clone()).style(theme.job),
            Cell::from(r.state.clone()).style(state),
        ]
    }

    fn sort_key(r: &JobRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Num(r.id as i128),
            1 => SortKey::Str(r.unit.clone()),
            2 => SortKey::Str(r.kind.clone()),
            _ => SortKey::Str(r.state.clone()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(1),
            Action::SortType => Some(2),
            Action::SortActive => Some(3),
            _ => None,
        }
    }

    fn matches(r: &JobRow, needle: &str) -> bool {
        r.unit.to_lowercase().contains(needle)
            || r.kind.contains(needle)
            || r.state.contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &JobRow, action: Action, ctx: &mut Ctx<'_>) {
        match action {
            Action::Select => {
                let path = ctx.store.unit_path(&row.unit);
                ctx.push(UnitDetailView::boxed(&row.unit, path, Tab::Status));
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::unit(ctx.scope, &row.unit))),
            Action::Stop => ctx.perform(UnitAction::CancelJob(row.id), &row.unit, true),
            _ => {}
        }
    }

    fn interested(signal: &SystemdSignal) -> bool {
        matches!(
            signal,
            SystemdSignal::JobNew { .. } | SystemdSignal::JobRemoved { .. }
        )
    }
}
