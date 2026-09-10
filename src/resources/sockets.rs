//! `:sockets` — `systemctl list-sockets`.

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
    store::{DataKind, SocketRow, Store},
    systemd::{fetch::FetchKind, journal::JournalSpec, watch::SystemdSignal},
    ui::theme::Theme,
};

pub struct SocketsResource;

const COLUMNS: &[Column] = &[
    Column::new("LISTEN", Constraint::Min(30)),
    Column::new("UNIT", Constraint::Min(24)),
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

impl Resource for SocketsResource {
    type Row = SocketRow;

    const ALIASES: &'static [&'static str] = &["socket", "so"];
    const NAME: &'static str = "sockets";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Sockets, Duration::from_secs(10))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Sockets
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<SocketRow> {
        store.sockets.clone()
    }

    fn key(row: &SocketRow) -> &str {
        &row.key
    }

    fn cells(r: &SocketRow, theme: &Theme) -> Vec<Cell<'static>> {
        vec![
            Cell::from(r.listen.clone()),
            Cell::from(r.unit.clone()).style(theme.name),
            Cell::from(r.activates.clone().unwrap_or_default()),
        ]
    }

    fn sort_key(r: &SocketRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.listen.clone()),
            1 => SortKey::Str(r.unit.clone()),
            _ => SortKey::Str(r.activates.clone().unwrap_or_default()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(1),
            Action::SortType => Some(0),
            _ => None,
        }
    }

    fn default_sort() -> usize {
        1
    }

    fn matches(r: &SocketRow, needle: &str) -> bool {
        r.unit.to_lowercase().contains(needle)
            || r.listen.to_lowercase().contains(needle)
            || r.activates
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &SocketRow, action: Action, ctx: &mut Ctx<'_>) {
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
