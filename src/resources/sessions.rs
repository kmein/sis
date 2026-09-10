//! `:sessions` — `loginctl list-sessions`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey, journal::JournalView};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, SessionRow, Store},
    systemd::{fetch::FetchKind, journal::JournalSpec},
    ui::{format, theme::Theme},
};

pub struct SessionsResource;

const COLUMNS: &[Column] = &[
    Column::new("SESSION", Constraint::Length(8)),
    Column::new("UID", Constraint::Length(6)).right(),
    Column::new("USER", Constraint::Length(12)),
    Column::new("SEAT", Constraint::Length(8)),
    Column::new("LEADER", Constraint::Length(8)).right(),
    Column::new("CLASS", Constraint::Length(12)),
    Column::new("TTY", Constraint::Length(8)),
    Column::new("IDLE", Constraint::Length(5)),
    Column::new("SINCE", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Status"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('x'), Action::Stop, "Terminate").confirm(),
    Binding::new(Key::ch('L'), Action::Kill, "Lock").confirm(),
];

impl Resource for SessionsResource {
    type Row = SessionRow;

    const ALIASES: &'static [&'static str] = &["session", "se"];
    const NAME: &'static str = "sessions";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Sessions, Duration::from_secs(5))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Sessions
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<SessionRow> {
        store.sessions.clone()
    }

    fn key(row: &SessionRow) -> &str {
        &row.session
    }

    fn cells(r: &SessionRow, theme: &Theme) -> Vec<Cell<'static>> {
        vec![
            Cell::from(r.session.clone()).style(theme.name),
            Cell::from(r.uid.to_string()).style(theme.dim),
            Cell::from(r.user.clone()),
            Cell::from(r.seat.clone().unwrap_or_default()),
            Cell::from(r.leader.map(|l| l.to_string()).unwrap_or_default()).style(theme.dim),
            Cell::from(r.class.clone()),
            Cell::from(r.tty.clone().unwrap_or_default()),
            Cell::from(if r.idle { "yes" } else { "no" }).style(if r.idle {
                theme.dim
            } else {
                theme.ok
            }),
            Cell::from(r.since.map(format::timestamp).unwrap_or_default()).style(theme.dim),
        ]
    }

    fn sort_key(r: &SessionRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Num(r.session.parse::<i128>().unwrap_or(0)),
            1 => SortKey::Num(r.uid as i128),
            2 => SortKey::Str(r.user.clone()),
            3 => SortKey::Str(r.seat.clone().unwrap_or_default()),
            4 => SortKey::Num(r.leader.unwrap_or(0) as i128),
            5 => SortKey::Str(r.class.clone()),
            6 => SortKey::Str(r.tty.clone().unwrap_or_default()),
            7 => SortKey::Num(r.idle as i128),
            _ => r.since.map_or(SortKey::None, |s| SortKey::Num(s as i128)),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(2),
            Action::SortType => Some(5),
            Action::SortActive => Some(0),
            _ => None,
        }
    }

    fn matches(r: &SessionRow, needle: &str) -> bool {
        r.user.to_lowercase().contains(needle)
            || r.class.contains(needle)
            || r.session == needle
            || r.tty.as_deref().unwrap_or("").contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &SessionRow, action: Action, ctx: &mut Ctx<'_>) {
        let id = row.session.as_str();
        match action {
            Action::Select => {
                ctx.exec(
                    Exec::new("loginctl", &["session-status", "--no-pager", id])
                        .show(format!("session {id}")),
                    false,
                );
            }
            Action::Logs => {
                ctx.push(JournalView::boxed(JournalSpec::matching(
                    ctx.scope,
                    format!("_AUDIT_SESSION={id}"),
                )));
            }
            Action::Stop => ctx.exec(Exec::new("loginctl", &["terminate-session", id]), true),
            Action::Kill => ctx.exec(Exec::new("loginctl", &["lock-session", id]), true),
            _ => {}
        }
    }
}
