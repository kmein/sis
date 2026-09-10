//! `:coredumps` — `coredumpctl list`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey, journal::JournalView};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{CoredumpRow, DataKind, Store},
    systemd::{fetch::FetchKind, journal::JournalSpec},
    ui::{format, theme::Theme},
};

pub struct CoredumpsResource;

const COLUMNS: &[Column] = &[
    Column::new("TIME", Constraint::Length(24)),
    Column::new("PID", Constraint::Length(8)).right(),
    Column::new("UID", Constraint::Length(6)).right(),
    Column::new("SIG", Constraint::Length(8)),
    Column::new("COREFILE", Constraint::Length(9)),
    Column::new("SIZE", Constraint::Length(8)).right(),
    Column::new("EXE", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Info"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs of pid"),
];

fn signal_name(sig: i64) -> String {
    match sig {
        1 => "SIGHUP".into(),
        2 => "SIGINT".into(),
        3 => "SIGQUIT".into(),
        4 => "SIGILL".into(),
        5 => "SIGTRAP".into(),
        6 => "SIGABRT".into(),
        7 => "SIGBUS".into(),
        8 => "SIGFPE".into(),
        9 => "SIGKILL".into(),
        11 => "SIGSEGV".into(),
        13 => "SIGPIPE".into(),
        15 => "SIGTERM".into(),
        24 => "SIGXCPU".into(),
        25 => "SIGXFSZ".into(),
        31 => "SIGSYS".into(),
        other => other.to_string(),
    }
}

impl Resource for CoredumpsResource {
    type Row = CoredumpRow;

    const ALIASES: &'static [&'static str] = &["coredump", "cores", "core"];
    const NAME: &'static str = "coredumps";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] =
            &[(FetchKind::Coredumps, Duration::from_secs(30))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Coredumps
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<CoredumpRow> {
        store.coredumps.clone()
    }

    fn key(row: &CoredumpRow) -> &str {
        &row.key
    }

    fn cells(r: &CoredumpRow, theme: &Theme) -> Vec<Cell<'static>> {
        let corefile = r.corefile.clone().unwrap_or_default();
        let core_style = match corefile.as_str() {
            "present" => theme.ok,
            "missing" | "none" => theme.dim,
            _ => theme.warn,
        };
        vec![
            Cell::from(format::timestamp(r.time)),
            Cell::from(r.pid.to_string()),
            Cell::from(r.uid.to_string()).style(theme.dim),
            Cell::from(r.sig.map(signal_name).unwrap_or_default()).style(theme.error),
            Cell::from(corefile).style(core_style),
            Cell::from(r.size.map(format::bytes).unwrap_or_default()).style(theme.dim),
            Cell::from(r.exe.clone().unwrap_or_default()).style(theme.name),
        ]
    }

    fn sort_key(r: &CoredumpRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Num(r.time as i128),
            1 => SortKey::Num(r.pid as i128),
            2 => SortKey::Num(r.uid as i128),
            3 => SortKey::Num(r.sig.unwrap_or(0) as i128),
            4 => SortKey::Str(r.corefile.clone().unwrap_or_default()),
            5 => r.size.map_or(SortKey::None, |s| SortKey::Num(s as i128)),
            _ => SortKey::Str(r.exe.clone().unwrap_or_default()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(6),
            Action::SortActive => Some(0),
            Action::SortType => Some(3),
            Action::SortMemory => Some(5),
            _ => None,
        }
    }

    fn matches(r: &CoredumpRow, needle: &str) -> bool {
        r.exe
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(needle)
            || r.pid.to_string() == needle
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &CoredumpRow, action: Action, ctx: &mut Ctx<'_>) {
        let pid = row.pid.to_string();
        match action {
            Action::Select => {
                let exec = Exec::new("coredumpctl", &["info", "--no-pager", &pid])
                    .show(format!("coredump {pid}"));
                ctx.exec(exec, false);
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::matching(
                ctx.scope,
                format!("_PID={pid}"),
            ))),
            _ => {}
        }
    }
}
