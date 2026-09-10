//! `:users` — `loginctl list-users`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey, journal::JournalView};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, Store, UserRow},
    systemd::{fetch::FetchKind, journal::JournalSpec},
    ui::theme::Theme,
};

pub struct UsersResource;

const COLUMNS: &[Column] = &[
    Column::new("UID", Constraint::Length(8)),
    Column::new("USER", Constraint::Min(16)),
    Column::new("LINGER", Constraint::Length(8)),
    Column::new("STATE", Constraint::Fill(1)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Status"),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('x'), Action::Stop, "Terminate").confirm(),
    Binding::new(Key::ch('L'), Action::Enable, "Toggle linger").confirm(),
];

impl Resource for UsersResource {
    type Row = UserRow;

    const ALIASES: &'static [&'static str] = &["user"];
    const NAME: &'static str = "users";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Users, Duration::from_secs(5))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Users
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<UserRow> {
        store.users.clone()
    }

    fn key(row: &UserRow) -> &str {
        &row.user
    }

    fn cells(r: &UserRow, theme: &Theme) -> Vec<Cell<'static>> {
        let state = match r.state.as_str() {
            "active" => theme.ok,
            "online" => Default::default(),
            _ => theme.dim,
        };
        vec![
            Cell::from(r.uid.to_string()).style(theme.dim),
            Cell::from(r.user.clone()).style(theme.name),
            Cell::from(if r.linger { "yes" } else { "no" }),
            Cell::from(r.state.clone()).style(state),
        ]
    }

    fn sort_key(r: &UserRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Num(r.uid as i128),
            1 => SortKey::Str(r.user.clone()),
            2 => SortKey::Num(r.linger as i128),
            _ => SortKey::Str(r.state.clone()),
        }
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(1),
            Action::SortActive => Some(3),
            _ => None,
        }
    }

    fn matches(r: &UserRow, needle: &str) -> bool {
        r.user.to_lowercase().contains(needle)
            || r.state.contains(needle)
            || r.uid.to_string() == needle
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &UserRow, action: Action, ctx: &mut Ctx<'_>) {
        let uid = row.uid.to_string();
        match action {
            Action::Select => {
                ctx.exec(
                    Exec::new("loginctl", &["user-status", "--no-pager", &uid])
                        .show(format!("user {}", row.user)),
                    false,
                );
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::matching(
                ctx.scope,
                format!("_UID={uid}"),
            ))),
            Action::Stop => ctx.exec(Exec::new("loginctl", &["terminate-user", &uid]), true),
            Action::Enable => {
                let verb = if row.linger {
                    "disable-linger"
                } else {
                    "enable-linger"
                };
                ctx.exec(Exec::new("loginctl", &[verb, &row.user]), true);
            }
            _ => {}
        }
    }
}
