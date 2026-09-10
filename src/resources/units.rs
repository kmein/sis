//! The main view: all units of the manager.

use std::time::Duration;

use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey};
use crate::{
    event::{Effect, Status},
    keys::{Action, Binding, Key},
    store::{DataKind, Store},
    systemd::{actions::UnitAction, fetch::FetchKind, unit::Unit, watch::SystemdSignal},
    ui::{format, theme::Theme},
};

/// Re-read a unit's properties after this long on screen.
const ENRICH_STALE: Duration = Duration::from_secs(10);

pub struct UnitsResource;

const COLUMNS: &[Column] = &[
    Column::new("NAME", Constraint::Min(30)),
    Column::new("TYPE", Constraint::Length(9)),
    Column::new("LOAD", Constraint::Length(9)),
    Column::new("ACTIVE", Constraint::Length(12)),
    Column::new("SUB", Constraint::Length(12)),
    Column::new("ENABLED", Constraint::Length(10)),
    Column::new("JOB", Constraint::Length(8)),
    Column::new("SINCE", Constraint::Length(7)).right(),
    Column::new("PID", Constraint::Length(7)).right(),
    Column::new("MEM", Constraint::Length(8)).right(),
    Column::new("DESCRIPTION", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(
        Key::code(crossterm::event::KeyCode::Enter),
        Action::Select,
        "Describe",
    ),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('s'), Action::Start, "Start"),
    Binding::new(Key::ch('x'), Action::Stop, "Stop").confirm(),
    Binding::new(Key::ch('r'), Action::Restart, "Restart").confirm(),
    Binding::new(Key::ch('R'), Action::Reload, "Reload"),
    Binding::new(Key::ch('e'), Action::Enable, "Enable"),
    Binding::new(Key::ch('d'), Action::Disable, "Disable").confirm(),
    Binding::new(Key::ch('m'), Action::Mask, "Mask").confirm(),
    Binding::new(Key::ch('M'), Action::Unmask, "Unmask"),
    Binding::new(Key::ch('f'), Action::ResetFailed, "Reset failed"),
    Binding::new(Key::ch('c'), Action::Cat, "Cat unit file"),
    Binding::new(Key::ctrl('k'), Action::Kill, "Kill").confirm(),
    Binding::new(Key::ch('D'), Action::DaemonReload, "Daemon reload").confirm(),
];

impl Resource for UnitsResource {
    type Row = Unit;

    const ALIASES: &'static [&'static str] = &["unit", "u"];
    const NAME: &'static str = "units";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[
            (FetchKind::Units, Duration::from_secs(5)),
            (FetchKind::UnitFiles, Duration::from_secs(30)),
        ];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        matches!(
            kind,
            DataKind::Units | DataKind::UnitFiles | DataKind::Enrichment
        )
    }

    fn rows(store: &Store, settings: &Settings) -> Vec<Unit> {
        store
            .units
            .iter()
            .filter(|u| settings.show_all || !u.is_boring())
            .filter(|u| settings.kind.as_ref().is_none_or(|k| &u.kind == k))
            .map(|u| {
                let mut u = u.clone();
                u.file_state = store.unit_files.get(&u.name).cloned();
                u.enrich = store.enrichment.get(&u.name).cloned();
                u
            })
            .collect()
    }

    fn key(row: &Unit) -> &str {
        &row.name
    }

    fn cells(u: &Unit, theme: &Theme) -> Vec<Cell<'static>> {
        let active = theme.active(&u.active);
        let enrich = u.enrich.as_ref();
        let since = enrich.map(|e| {
            if u.is_failed() {
                e.state_change
            } else {
                e.active_enter
            }
        });
        vec![
            Cell::from(u.name.clone()).style(theme.name),
            Cell::from(u.kind.as_str().to_owned()),
            Cell::from(u.load.as_str().to_owned()).style(theme.load(&u.load)),
            Cell::from(u.active.as_str().to_owned()).style(active),
            Cell::from(u.sub.clone()).style(active),
            Cell::from(u.file_state.clone().unwrap_or_default())
                .style(theme.file_state(u.file_state.as_deref())),
            Cell::from(u.job.as_ref().map(|(_, ty)| ty.clone()).unwrap_or_default())
                .style(theme.job),
            Cell::from(format::age(since.unwrap_or(0))).style(theme.dim),
            Cell::from(
                enrich
                    .and_then(|e| e.main_pid)
                    .filter(|&p| p != 0)
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
            ),
            Cell::from(
                enrich
                    .and_then(|e| e.memory_current)
                    .map(format::bytes)
                    .unwrap_or_default(),
            ),
            Cell::from(u.description.clone()).style(theme.dim),
        ]
    }

    fn sort_key(u: &Unit, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(u.name.clone()),
            1 => SortKey::Str(u.kind.as_str().to_owned()),
            2 => SortKey::Str(u.load.as_str().to_owned()),
            3 => SortKey::Str(u.active.as_str().to_owned()),
            4 => SortKey::Str(u.sub.clone()),
            5 => u.file_state.clone().map_or(SortKey::None, SortKey::Str),
            6 => u
                .job
                .as_ref()
                .map_or(SortKey::None, |(_, t)| SortKey::Str(t.clone())),
            7 => u
                .enrich
                .as_ref()
                .map_or(SortKey::None, |e| SortKey::Num(e.active_enter as i128)),
            8 => u
                .enrich
                .as_ref()
                .and_then(|e| e.main_pid)
                .map_or(SortKey::None, |p| SortKey::Num(p as i128)),
            9 => u
                .enrich
                .as_ref()
                .and_then(|e| e.memory_current)
                .map_or(SortKey::None, |m| SortKey::Num(m as i128)),
            _ => SortKey::Str(u.description.clone()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortType => Some(1),
            Action::SortLoad => Some(2),
            Action::SortActive => Some(3),
            Action::SortMemory => Some(9),
            _ => None,
        }
    }

    fn matches(u: &Unit, needle: &str) -> bool {
        u.name.to_lowercase().contains(needle)
            || u.description.to_lowercase().contains(needle)
            || u.active.as_str().contains(needle)
            || u.sub.contains(needle)
            || u.load.as_str().contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &Unit, action: Action, ctx: &mut Ctx<'_>) {
        let confirm = BINDINGS.iter().any(|b| b.action == action && b.confirm);
        let unit_action = match action {
            Action::Start => UnitAction::Start,
            Action::Stop => UnitAction::Stop,
            Action::Restart => UnitAction::Restart,
            Action::Reload => UnitAction::Reload,
            Action::Enable => UnitAction::Enable,
            Action::Disable => UnitAction::Disable,
            Action::Mask => UnitAction::Mask,
            Action::Unmask => UnitAction::Unmask,
            Action::ResetFailed => UnitAction::ResetFailed,
            Action::Kill => UnitAction::Kill,
            Action::DaemonReload => UnitAction::DaemonReload,
            other => {
                ctx.status(Status::info(format!(
                    "{other:?} {} is not implemented yet",
                    row.name
                )));
                return;
            }
        };
        ctx.perform(unit_action, &row.name, confirm);
    }

    fn enrich(rows: &[&Unit], ctx: &mut Ctx<'_>) {
        let now = ctx.now;
        let targets: Vec<_> = rows
            .iter()
            .filter(|u| {
                u.enrich
                    .as_ref()
                    .is_none_or(|e| now.duration_since(e.fetched_at) >= ENRICH_STALE)
            })
            .map(|u| (u.name.clone(), u.path.clone()))
            .collect();
        if !targets.is_empty() {
            ctx.effect(Effect::Enrich(targets));
        }
    }

    fn interested(signal: &SystemdSignal) -> bool {
        !matches!(signal, SystemdSignal::Reloading(_))
    }
}
