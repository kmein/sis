//! `:bus` — `busctl list`.

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
    store::{BusRow, DataKind, Store},
    systemd::{Scope, fetch::FetchKind},
    ui::theme::Theme,
};

pub struct BusResource;

const COLUMNS: &[Column] = &[
    Column::new("NAME", Constraint::Min(30)),
    Column::new("PID", Constraint::Length(8)),
    Column::new("PROCESS", Constraint::Length(18)),
    Column::new("USER", Constraint::Length(12)),
    Column::new("CONNECTION", Constraint::Length(12)),
    Column::new("UNIT", Constraint::Min(20)),
    Column::new("SESSION", Constraint::Length(8)),
    Column::new("DESCRIPTION", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Tree"),
    Binding::new(Key::ch('i'), Action::Cat, "Introspect /"),
    Binding::new(Key::ch('s'), Action::Logs, "Status"),
    Binding::new(Key::ch('U'), Action::Start, "Describe unit"),
];

impl Resource for BusResource {
    type Row = BusRow;

    const ALIASES: &'static [&'static str] = &["busctl", "dbus"];
    const NAME: &'static str = "bus";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Bus, Duration::from_secs(10))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Bus
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<BusRow> {
        store.bus.clone()
    }

    fn key(row: &BusRow) -> &str {
        &row.name
    }

    fn cells(r: &BusRow, theme: &Theme) -> Vec<Cell<'static>> {
        let name_style = if r.name.starts_with(':') {
            theme.dim
        } else {
            theme.name
        };
        vec![
            Cell::from(r.name.clone()).style(name_style),
            Cell::from(r.pid.map(|p| p.to_string()).unwrap_or_default()),
            Cell::from(r.process.clone().unwrap_or_default()),
            Cell::from(r.user.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.connection.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.unit.clone().unwrap_or_default()),
            Cell::from(r.session.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.description.clone().unwrap_or_default()).style(theme.dim),
        ]
    }

    fn sort_key(r: &BusRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.name.clone()),
            1 => SortKey::Num(r.pid.unwrap_or(0) as i128),
            2 => SortKey::Str(r.process.clone().unwrap_or_default()),
            3 => SortKey::Str(r.user.clone().unwrap_or_default()),
            4 => SortKey::Str(r.connection.clone().unwrap_or_default()),
            5 => SortKey::Str(r.unit.clone().unwrap_or_default()),
            6 => SortKey::Str(r.session.clone().unwrap_or_default()),
            _ => SortKey::Str(r.description.clone().unwrap_or_default()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortType => Some(2),
            Action::SortLoad => Some(5),
            _ => None,
        }
    }

    fn matches(r: &BusRow, needle: &str) -> bool {
        r.name.to_lowercase().contains(needle)
            || r.process
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
            || r.unit
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &BusRow, action: Action, ctx: &mut Ctx<'_>) {
        let name = row.name.as_str();
        let user = ctx.scope == Scope::User;
        let mut base: Vec<&str> = if user { vec!["--user"] } else { Vec::new() };
        match action {
            Action::Select => {
                base.extend(["tree", "--no-pager", name]);
                ctx.exec(
                    Exec::new("busctl", &base).show(format!("tree {name}")),
                    false,
                );
            }
            Action::Cat => {
                base.extend(["introspect", "--no-pager", name, "/"]);
                ctx.exec(
                    Exec::new("busctl", &base).show(format!("introspect {name} /")),
                    false,
                );
            }
            Action::Logs => {
                base.extend(["status", "--no-pager", name]);
                ctx.exec(
                    Exec::new("busctl", &base).show(format!("status {name}")),
                    false,
                );
            }
            Action::Start => {
                if let Some(unit) = &row.unit {
                    let path = ctx.store.unit_path(unit);
                    ctx.push(UnitDetailView::boxed(unit, path, Tab::Status));
                }
            }
            _ => {}
        }
    }
}
