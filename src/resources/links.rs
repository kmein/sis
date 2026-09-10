//! `:links` — `networkctl list`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey, journal::JournalView};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, LinkRow, Store},
    systemd::{fetch::FetchKind, journal::JournalSpec},
    ui::theme::Theme,
};

pub struct LinksResource;

const COLUMNS: &[Column] = &[
    Column::new("IDX", Constraint::Length(4)),
    Column::new("LINK", Constraint::Min(14)),
    Column::new("TYPE", Constraint::Length(10)),
    Column::new("OPERATIONAL", Constraint::Length(12)),
    Column::new("SETUP", Constraint::Length(12)),
    Column::new("CARRIER", Constraint::Length(8)),
    Column::new("ONLINE", Constraint::Length(8)),
    Column::new("MTU", Constraint::Length(6)),
    Column::new("DRIVER", Constraint::Length(12)),
    Column::new("NETWORK FILE", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Status"),
    Binding::new(Key::ch('l'), Action::Logs, "networkd logs"),
    Binding::new(Key::ch('r'), Action::Restart, "Reconfigure").confirm(),
    Binding::new(Key::ch('s'), Action::Start, "Up"),
    Binding::new(Key::ch('x'), Action::Stop, "Down").confirm(),
];

impl Resource for LinksResource {
    type Row = LinkRow;

    const ALIASES: &'static [&'static str] = &["link", "net", "network"];
    const NAME: &'static str = "links";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Links, Duration::from_secs(5))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Links
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<LinkRow> {
        store.links.clone()
    }

    fn key(row: &LinkRow) -> &str {
        &row.name
    }

    fn cells(r: &LinkRow, theme: &Theme) -> Vec<Cell<'static>> {
        let oper = r.operational_state.clone().unwrap_or_default();
        let oper_style = match oper.as_str() {
            "routable" | "enslaved" => theme.ok,
            "carrier" | "degraded" | "degraded-carrier" | "dormant" => theme.warn,
            "off" | "no-carrier" | "missing" => theme.dim,
            _ => Default::default(),
        };
        let setup = r.administrative_state.clone().unwrap_or_default();
        let setup_style = match setup.as_str() {
            "configured" => theme.ok,
            "unmanaged" => theme.dim,
            "failed" => theme.error,
            _ => theme.warn,
        };
        vec![
            Cell::from(r.index.to_string()).style(theme.dim),
            Cell::from(r.name.clone()).style(theme.name),
            Cell::from(r.kind.clone().unwrap_or_default()),
            Cell::from(oper).style(oper_style),
            Cell::from(setup).style(setup_style),
            Cell::from(r.carrier_state.clone().unwrap_or_default()),
            Cell::from(r.online_state.clone().unwrap_or_default()),
            Cell::from(r.mtu.map(|m| m.to_string()).unwrap_or_default()).style(theme.dim),
            Cell::from(r.driver.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.network_file.clone().unwrap_or_default()).style(theme.dim),
        ]
    }

    fn sort_key(r: &LinkRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Num(r.index as i128),
            1 => SortKey::Str(r.name.clone()),
            2 => SortKey::Str(r.kind.clone().unwrap_or_default()),
            3 => SortKey::Str(r.operational_state.clone().unwrap_or_default()),
            4 => SortKey::Str(r.administrative_state.clone().unwrap_or_default()),
            5 => SortKey::Str(r.carrier_state.clone().unwrap_or_default()),
            6 => SortKey::Str(r.online_state.clone().unwrap_or_default()),
            7 => SortKey::Num(r.mtu.unwrap_or(0) as i128),
            8 => SortKey::Str(r.driver.clone().unwrap_or_default()),
            _ => SortKey::Str(r.network_file.clone().unwrap_or_default()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(1),
            Action::SortType => Some(2),
            Action::SortActive => Some(3),
            Action::SortLoad => Some(4),
            _ => None,
        }
    }

    fn matches(r: &LinkRow, needle: &str) -> bool {
        r.name.to_lowercase().contains(needle)
            || r.kind.as_deref().unwrap_or("").contains(needle)
            || r.operational_state
                .as_deref()
                .unwrap_or("")
                .contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &LinkRow, action: Action, ctx: &mut Ctx<'_>) {
        let name = row.name.as_str();
        match action {
            Action::Select => {
                ctx.exec(
                    Exec::new("networkctl", &["status", "--no-pager", name])
                        .show(format!("link {name}")),
                    false,
                );
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::unit(
                ctx.scope,
                "systemd-networkd.service",
            ))),
            Action::Restart => ctx.exec(Exec::new("networkctl", &["reconfigure", name]), true),
            Action::Start => ctx.exec(Exec::new("networkctl", &["up", name]), false),
            Action::Stop => ctx.exec(Exec::new("networkctl", &["down", name]), true),
            _ => {}
        }
    }
}
