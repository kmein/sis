//! `:userdb` and `:groups` — `userdbctl user` / `userdbctl group`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, GroupRow, Store, UserdbRow},
    systemd::fetch::FetchKind,
    ui::theme::Theme,
};

pub struct UserdbResource;

const USER_COLUMNS: &[Column] = &[
    Column::new("NAME", Constraint::Min(16)),
    Column::new("UID", Constraint::Length(8)),
    Column::new("GID", Constraint::Length(8)),
    Column::new("REAL NAME", Constraint::Min(16)),
    Column::new("HOME", Constraint::Min(16)),
    Column::new("SHELL", Constraint::Fill(1)),
];

const USER_BINDINGS: &[Binding] = &[Binding::new(
    Key::code(KeyCode::Enter),
    Action::Select,
    "Record",
)];

impl Resource for UserdbResource {
    type Row = UserdbRow;

    const ALIASES: &'static [&'static str] = &["userdbctl", "passwd"];
    const NAME: &'static str = "userdb";

    fn columns() -> &'static [Column] {
        USER_COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Userdb, Duration::from_secs(60))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Userdb
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<UserdbRow> {
        store.userdb.clone()
    }

    fn key(row: &UserdbRow) -> &str {
        &row.user_name
    }

    fn cells(r: &UserdbRow, theme: &Theme) -> Vec<Cell<'static>> {
        let system = r.uid < 1000 || r.disposition.as_deref() == Some("system");
        vec![
            Cell::from(r.user_name.clone()).style(if system { theme.dim } else { theme.name }),
            Cell::from(r.uid.to_string()),
            Cell::from(r.gid.to_string()).style(theme.dim),
            Cell::from(r.real_name.clone().unwrap_or_default()),
            Cell::from(r.home_directory.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.shell.clone().unwrap_or_default()).style(theme.dim),
        ]
    }

    fn sort_key(r: &UserdbRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.user_name.clone()),
            1 => SortKey::Num(r.uid as i128),
            2 => SortKey::Num(r.gid as i128),
            3 => SortKey::Str(r.real_name.clone().unwrap_or_default()),
            4 => SortKey::Str(r.home_directory.clone().unwrap_or_default()),
            _ => SortKey::Str(r.shell.clone().unwrap_or_default()),
        }
    }

    fn default_sort() -> (usize, bool) {
        (1, false)
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortActive => Some(1),
            _ => None,
        }
    }

    fn matches(r: &UserdbRow, needle: &str) -> bool {
        r.user_name.to_lowercase().contains(needle)
            || r.real_name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
            || r.uid.to_string() == needle
    }

    fn bindings() -> &'static [Binding] {
        USER_BINDINGS
    }

    fn on_action(row: &UserdbRow, action: Action, ctx: &mut Ctx<'_>) {
        if action == Action::Select {
            let exec = Exec::new("userdbctl", &["user", "--no-pager", &row.user_name])
                .show(format!("user {}", row.user_name));
            ctx.exec(exec, false);
        }
    }
}

pub struct GroupsResource;

const GROUP_COLUMNS: &[Column] = &[
    Column::new("NAME", Constraint::Min(16)),
    Column::new("GID", Constraint::Length(8)),
    Column::new("DESCRIPTION", Constraint::Fill(1)),
];

impl Resource for GroupsResource {
    type Row = GroupRow;

    const ALIASES: &'static [&'static str] = &["group"];
    const NAME: &'static str = "groups";

    fn columns() -> &'static [Column] {
        GROUP_COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Groups, Duration::from_secs(60))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Groups
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<GroupRow> {
        store.groups.clone()
    }

    fn key(row: &GroupRow) -> &str {
        &row.group_name
    }

    fn cells(r: &GroupRow, theme: &Theme) -> Vec<Cell<'static>> {
        let system = r.gid < 1000 || r.disposition.as_deref() == Some("system");
        vec![
            Cell::from(r.group_name.clone()).style(if system { theme.dim } else { theme.name }),
            Cell::from(r.gid.to_string()),
            Cell::from(r.description.clone().unwrap_or_default()).style(theme.dim),
        ]
    }

    fn sort_key(r: &GroupRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.group_name.clone()),
            1 => SortKey::Num(r.gid as i128),
            _ => SortKey::Str(r.description.clone().unwrap_or_default()),
        }
    }

    fn default_sort() -> (usize, bool) {
        (1, false)
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortActive => Some(1),
            _ => None,
        }
    }

    fn matches(r: &GroupRow, needle: &str) -> bool {
        r.group_name.to_lowercase().contains(needle) || r.gid.to_string() == needle
    }

    fn bindings() -> &'static [Binding] {
        USER_BINDINGS
    }

    fn on_action(row: &GroupRow, action: Action, ctx: &mut Ctx<'_>) {
        if action == Action::Select {
            let exec = Exec::new("userdbctl", &["group", "--no-pager", &row.group_name])
                .show(format!("group {}", row.group_name));
            ctx.exec(exec, false);
        }
    }
}
