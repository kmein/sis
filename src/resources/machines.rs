//! `:machines` — `machinectl list`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey, journal::JournalView};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, MachineRow, Store},
    systemd::{
        fetch::FetchKind,
        journal::{JournalSpec, JournalTarget},
    },
    ui::theme::Theme,
};

pub struct MachinesResource;

const COLUMNS: &[Column] = &[
    Column::new("MACHINE", Constraint::Min(20)),
    Column::new("CLASS", Constraint::Length(10)),
    Column::new("SERVICE", Constraint::Length(16)),
    Column::new("OS", Constraint::Length(12)),
    Column::new("VERSION", Constraint::Length(12)),
    Column::new("ADDRESSES", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Status"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('s'), Action::Start, "Start"),
    Binding::new(Key::ch('x'), Action::Stop, "Poweroff").confirm(),
    Binding::new(Key::ctrl('k'), Action::Kill, "Terminate").confirm(),
    Binding::new(Key::ch('r'), Action::Restart, "Reboot").confirm(),
    Binding::new(Key::ch('!'), Action::Shell, "Shell"),
];

fn addresses(r: &MachineRow) -> String {
    match &r.addresses {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect::<Vec<_>>()
            .join(" "),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

impl Resource for MachinesResource {
    type Row = MachineRow;

    const ALIASES: &'static [&'static str] = &["machine", "m"];
    const NAME: &'static str = "machines";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Machines, Duration::from_secs(5))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Machines
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<MachineRow> {
        store.machines.clone()
    }

    fn key(row: &MachineRow) -> &str {
        &row.machine
    }

    fn cells(r: &MachineRow, theme: &Theme) -> Vec<Cell<'static>> {
        vec![
            Cell::from(r.machine.clone()).style(theme.name),
            Cell::from(r.class.clone().unwrap_or_default()),
            Cell::from(r.service.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.os.clone().unwrap_or_default()),
            Cell::from(r.version.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(addresses(r)),
        ]
    }

    fn sort_key(r: &MachineRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.machine.clone()),
            1 => SortKey::Str(r.class.clone().unwrap_or_default()),
            2 => SortKey::Str(r.service.clone().unwrap_or_default()),
            3 => SortKey::Str(r.os.clone().unwrap_or_default()),
            4 => SortKey::Str(r.version.clone().unwrap_or_default()),
            _ => SortKey::Str(addresses(r)),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortType => Some(1),
            _ => None,
        }
    }

    fn matches(r: &MachineRow, needle: &str) -> bool {
        r.machine.to_lowercase().contains(needle)
            || r.os
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &MachineRow, action: Action, ctx: &mut Ctx<'_>) {
        let m = row.machine.as_str();
        match action {
            Action::Select => {
                ctx.exec(
                    Exec::new("machinectl", &["status", "--no-pager", m])
                        .show(format!("machine {m}")),
                    false,
                );
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec {
                scope: ctx.scope,
                target: JournalTarget::Machine(m.to_owned()),
                lines: 500,
            })),
            Action::Start => ctx.exec(Exec::new("machinectl", &["start", m]), false),
            Action::Stop => ctx.exec(Exec::new("machinectl", &["poweroff", m]), true),
            Action::Kill => ctx.exec(Exec::new("machinectl", &["terminate", m]), true),
            Action::Shell => ctx.interactive(Exec::new("machinectl", &["shell", m])),
            Action::Restart => ctx.exec(Exec::new("machinectl", &["reboot", m]), true),
            _ => {}
        }
    }
}
