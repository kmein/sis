//! `:security` — `systemd-analyze security`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{
    Column, Ctx, Resource, Settings, SortKey,
    unit_detail::{Tab, UnitDetailView},
};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, SecurityRow, Store},
    systemd::{Scope, fetch::FetchKind},
    ui::theme::Theme,
};

pub struct SecurityResource;

const COLUMNS: &[Column] = &[
    Column::new("UNIT", Constraint::Min(30)),
    Column::new("EXPOSURE", Constraint::Length(9)),
    Column::new("PREDICATE", Constraint::Length(10)),
    Column::new("", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Analysis"),
    Binding::new(Key::ch('c'), Action::Cat, "Describe unit"),
];

fn exposure(r: &SecurityRow) -> f64 {
    r.exposure.parse().unwrap_or(0.0)
}

impl Resource for SecurityResource {
    type Row = SecurityRow;

    const ALIASES: &'static [&'static str] = &["sec", "analyze-security"];
    const NAME: &'static str = "security";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] =
            &[(FetchKind::Security, Duration::from_secs(300))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Security
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<SecurityRow> {
        store.security.values().cloned().collect()
    }

    fn key(row: &SecurityRow) -> &str {
        &row.unit
    }

    fn cells(r: &SecurityRow, theme: &Theme) -> Vec<Cell<'static>> {
        let style = theme.exposure(Some(&r.predicate));
        vec![
            Cell::from(r.unit.clone()).style(theme.name),
            Cell::from(r.exposure.clone()).style(style),
            Cell::from(r.predicate.clone()).style(style),
            Cell::from(r.happy.clone()),
        ]
    }

    fn sort_key(r: &SecurityRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.unit.clone()),
            1 | 2 => SortKey::Num((exposure(r) * 10.0) as i128),
            _ => SortKey::Str(r.happy.clone()),
        }
    }

    fn default_sort() -> (usize, bool) {
        (1, true)
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortActive | Action::SortMemory => Some(1),
            _ => None,
        }
    }

    fn matches(r: &SecurityRow, needle: &str) -> bool {
        r.unit.to_lowercase().contains(needle) || r.predicate.to_lowercase().contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &SecurityRow, action: Action, ctx: &mut Ctx<'_>) {
        match action {
            Action::Select => {
                let mut args = vec!["security", "--no-pager"];
                if ctx.scope == Scope::User {
                    args.push("--user");
                }
                args.push(&row.unit);
                ctx.exec(
                    Exec::new("systemd-analyze", &args).show(format!("security {}", row.unit)),
                    false,
                );
            }
            Action::Cat => {
                let path = ctx.store.unit_path(&row.unit);
                ctx.push(UnitDetailView::boxed(&row.unit, path, Tab::Status));
            }
            _ => {}
        }
    }
}
