//! `:blame` — `systemd-analyze blame`.

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
    store::{BlameRow, DataKind, Store},
    systemd::{fetch::FetchKind, journal::JournalSpec},
    ui::theme::Theme,
};

pub struct BlameResource;

const COLUMNS: &[Column] = &[
    Column::new("TIME", Constraint::Length(16)),
    Column::new("UNIT", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Describe"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
];

impl Resource for BlameResource {
    type Row = BlameRow;

    const ALIASES: &'static [&'static str] = &["analyze-blame", "startup"];
    const NAME: &'static str = "blame";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Blame, Duration::from_secs(300))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Blame
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<BlameRow> {
        store.blame.clone()
    }

    fn key(row: &BlameRow) -> &str {
        &row.unit
    }

    fn cells(r: &BlameRow, theme: &Theme) -> Vec<Cell<'static>> {
        let style = if r.usec >= 10_000_000 {
            theme.error
        } else if r.usec >= 1_000_000 {
            theme.warn
        } else {
            theme.dim
        };
        vec![
            Cell::from(r.text.clone()).style(style),
            Cell::from(r.unit.clone()).style(theme.name),
        ]
    }

    fn sort_key(r: &BlameRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Num(r.usec as i128),
            _ => SortKey::Str(r.unit.clone()),
        }
    }

    fn default_sort() -> (usize, bool) {
        (0, true)
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(1),
            Action::SortActive | Action::SortMemory => Some(0),
            _ => None,
        }
    }

    fn matches(r: &BlameRow, needle: &str) -> bool {
        r.unit.to_lowercase().contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &BlameRow, action: Action, ctx: &mut Ctx<'_>) {
        match action {
            Action::Select => {
                let path = ctx.store.unit_path(&row.unit);
                ctx.push(UnitDetailView::boxed(&row.unit, path, Tab::Status));
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::unit(ctx.scope, &row.unit))),
            _ => {}
        }
    }
}
