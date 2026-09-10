//! `:plot` — the boot timeline from `systemd-analyze plot --json`.

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
    store::{DataKind, PlotRow, Store},
    systemd::{fetch::FetchKind, journal::JournalSpec},
    ui::{format, theme::Theme},
};

pub struct PlotResource;

const COLUMNS: &[Column] = &[
    Column::new("UNIT", Constraint::Min(30)),
    Column::new("ACTIVATING", Constraint::Length(12)),
    Column::new("ACTIVATED", Constraint::Length(12)),
    Column::new("TOOK", Constraint::Length(12)),
    Column::new("DEACTIVATED", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Describe"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('C'), Action::CriticalChain, "Critical chain"),
];

fn at(usec: Option<u64>) -> String {
    usec.filter(|u| *u != 0)
        .map(|u| format!("@{}", format::timespan(u)))
        .unwrap_or_default()
}

impl Resource for PlotResource {
    type Row = PlotRow;

    const ALIASES: &'static [&'static str] = &["timeline", "boot-timeline"];
    const NAME: &'static str = "plot";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Plot, Duration::from_secs(300))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Plot
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<PlotRow> {
        store.plot.clone()
    }

    fn key(row: &PlotRow) -> &str {
        &row.name
    }

    fn cells(r: &PlotRow, theme: &Theme) -> Vec<Cell<'static>> {
        let took = r.time.unwrap_or(0);
        let took_style = if took >= 10_000_000 {
            theme.error
        } else if took >= 1_000_000 {
            theme.warn
        } else {
            theme.dim
        };
        vec![
            Cell::from(r.name.clone()).style(theme.name),
            Cell::from(at(r.activating)),
            Cell::from(at(r.activated)).style(theme.ok),
            Cell::from(if took == 0 {
                String::new()
            } else {
                format!("+{}", format::timespan(took))
            })
            .style(took_style),
            Cell::from(at(r.deactivated)).style(theme.dim),
        ]
    }

    fn sort_key(r: &PlotRow, col: usize) -> SortKey {
        let num = |v: Option<u64>| {
            v.filter(|u| *u != 0)
                .map_or(SortKey::None, |u| SortKey::Num(u as i128))
        };
        match col {
            0 => SortKey::Str(r.name.clone()),
            1 => num(r.activating),
            2 => num(r.activated),
            3 => num(r.time),
            _ => num(r.deactivated),
        }
    }

    fn default_sort() -> (usize, bool) {
        (1, false)
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortActive => Some(1),
            Action::SortMemory => Some(3),
            _ => None,
        }
    }

    fn matches(r: &PlotRow, needle: &str) -> bool {
        r.name.to_lowercase().contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &PlotRow, action: Action, ctx: &mut Ctx<'_>) {
        match action {
            Action::Select => {
                let path = ctx.store.unit_path(&row.name);
                ctx.push(UnitDetailView::boxed(&row.name, path, Tab::Status));
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::unit(ctx.scope, &row.name))),
            Action::CriticalChain => super::units::analyze(&row.name, "critical-chain", ctx),
            _ => {}
        }
    }
}
