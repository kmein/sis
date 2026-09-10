//! The main view: all units of the manager.

use std::time::Duration;

use ratatui::{layout::Constraint, widgets::Cell};

use super::{
    Column, Ctx, Resource, Settings, SortKey,
    journal::JournalView,
    text::TextView,
    unit_detail::{Tab, UnitDetailView},
};
use crate::{
    event::{Effect, Exec, Status},
    keys::{Action, Binding, Key},
    store::{DataKind, Store},
    systemd::{
        Scope, actions::UnitAction, fetch::FetchKind, journal::JournalSpec, unit::Unit,
        watch::SystemdSignal,
    },
    ui::{format, theme::Theme},
};

/// Map a key action onto a unit action and run it; shared by every view
/// that acts on a unit.
/// Show `systemd-analyze VERB UNIT` in a text view.
pub fn analyze(unit: &str, verb: &str, ctx: &mut Ctx<'_>) {
    let mut args: Vec<&str> = Vec::new();
    if ctx.scope == Scope::User {
        args.push("--user");
    }
    args.extend([verb, "--no-pager", unit]);
    let title = format!("{verb} {unit}");
    ctx.push(TextView::command(
        title,
        Exec::new("systemd-analyze", &args),
    ));
}

/// `systemd-analyze unit-shell` / `unit-gdb`: needs root, takes the terminal.
pub fn interactive(unit: &str, verb: &str, ctx: &mut Ctx<'_>) {
    let mut args: Vec<&str> = Vec::new();
    if ctx.scope == Scope::User {
        args.push("--user");
    }
    args.extend([verb, unit]);
    ctx.interactive(Exec::new("systemd-analyze", &args));
}

/// The analysis keys shared by the units and detail views; `true` if handled.
pub fn analysis_action(unit: &str, action: Action, ctx: &mut Ctx<'_>) -> bool {
    match action {
        Action::Shell => interactive(unit, "unit-shell", ctx),
        Action::Debug => interactive(unit, "unit-gdb", ctx),
        Action::CriticalChain => analyze(unit, "critical-chain", ctx),
        Action::Verify => analyze(unit, "verify", ctx),
        Action::Dump => analyze(unit, "dump", ctx),
        _ => return false,
    }
    true
}

pub fn perform(unit: &str, action: Action, confirm: bool, ctx: &mut Ctx<'_>) {
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
                "{other:?} {unit} is not implemented yet"
            )));
            return;
        }
    };
    ctx.perform(unit_action, unit, confirm);
}

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
    Column::new("SINCE", Constraint::Length(7)),
    Column::new("PID", Constraint::Length(7)),
    Column::new("MEM", Constraint::Length(8)),
    Column::new("DESCRIPTION", Constraint::Fill(1)),
    Column::new("SEC", Constraint::Length(12)),
];

/// Index of the optional security column.
const SEC: usize = 11;

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
    Binding::new(Key::ch('!'), Action::Shell, "Unit shell"),
    Binding::new(Key::ctrl('g'), Action::Debug, "Unit gdb").quiet(),
    Binding::new(Key::ch('C'), Action::CriticalChain, "Critical chain"),
    Binding::new(Key::ch('V'), Action::Verify, "Verify").quiet(),
    Binding::new(Key::ch('Y'), Action::Dump, "Dump state").quiet(),
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
                u.exposure = store
                    .security
                    .get(&u.name)
                    .map(|s| format!("{} {}", s.exposure, s.predicate));
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
            if enrich.is_some_and(|e| e.need_daemon_reload) {
                Cell::from(format!("{}*", u.load)).style(theme.warn)
            } else {
                Cell::from(u.load.as_str().to_owned()).style(theme.load(&u.load))
            },
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
            Cell::from(u.exposure.clone().unwrap_or_default())
                .style(theme.exposure(u.exposure.as_deref())),
        ]
    }

    fn active_columns(settings: &Settings) -> Vec<usize> {
        (0..COLUMNS.len())
            .filter(|&i| i != SEC || settings.security)
            .collect()
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
        match action {
            Action::Select => ctx.push(UnitDetailView::boxed(
                &row.name,
                Some(row.path.clone()),
                Tab::Status,
            )),
            Action::Cat => ctx.push(UnitDetailView::boxed(
                &row.name,
                Some(row.path.clone()),
                Tab::File,
            )),
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::unit(ctx.scope, &row.name))),
            other if analysis_action(&row.name, other, ctx) => {}
            other => perform(&row.name, other, confirm, ctx),
        }
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
