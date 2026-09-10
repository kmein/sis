//! `:boot` — `bootctl list`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey, text::TextView};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{BootRow, DataKind, Store},
    systemd::fetch::FetchKind,
    ui::theme::Theme,
};

pub struct BootResource;

const COLUMNS: &[Column] = &[
    Column::new("ID", Constraint::Min(30)),
    Column::new("TITLE", Constraint::Min(20)),
    Column::new("VERSION", Constraint::Length(20)),
    Column::new("TYPE", Constraint::Length(8)),
    Column::new("SOURCE", Constraint::Length(8)),
    Column::new("DEFAULT", Constraint::Length(8)),
    Column::new("SELECTED", Constraint::Length(8)),
];

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Enter), Action::Select, "Entry"),
    Binding::new(Key::ch('D'), Action::Enable, "Set default").confirm(),
    Binding::new(Key::ch('O'), Action::Start, "Boot once").confirm(),
];

impl Resource for BootResource {
    type Row = BootRow;

    const ALIASES: &'static [&'static str] = &["bootctl", "entries"];
    const NAME: &'static str = "boot";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Boot, Duration::from_secs(60))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Boot
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<BootRow> {
        store.boot.clone()
    }

    fn key(row: &BootRow) -> &str {
        &row.id
    }

    fn cells(r: &BootRow, theme: &Theme) -> Vec<Cell<'static>> {
        let flag = |b: bool| {
            if b {
                Cell::from("yes").style(theme.ok)
            } else {
                Cell::from("")
            }
        };
        vec![
            Cell::from(r.id.clone()).style(theme.name),
            Cell::from(
                r.show_title
                    .clone()
                    .or_else(|| r.title.clone())
                    .unwrap_or_default(),
            ),
            Cell::from(r.version.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.kind.clone().unwrap_or_default()).style(theme.dim),
            Cell::from(r.source.clone().unwrap_or_default()).style(theme.dim),
            flag(r.is_default),
            flag(r.is_selected),
        ]
    }

    fn sort_key(r: &BootRow, col: usize) -> SortKey {
        match col {
            0 => SortKey::Str(r.id.clone()),
            1 => SortKey::Str(r.show_title.clone().unwrap_or_default()),
            2 => SortKey::Str(r.version.clone().unwrap_or_default()),
            3 => SortKey::Str(r.kind.clone().unwrap_or_default()),
            4 => SortKey::Str(r.source.clone().unwrap_or_default()),
            5 => SortKey::Num(r.is_default as i128),
            _ => SortKey::Num(r.is_selected as i128),
        }
    }

    fn default_sort() -> (usize, bool) {
        (0, true)
    }

    fn sort_column(action: Action) -> Option<usize> {
        match action {
            Action::SortName => Some(0),
            Action::SortType => Some(3),
            _ => None,
        }
    }

    fn matches(r: &BootRow, needle: &str) -> bool {
        r.id.to_lowercase().contains(needle)
            || r.show_title
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &BootRow, action: Action, ctx: &mut Ctx<'_>) {
        match action {
            Action::Select => {
                let mut text = format!(
                    "id: {}\ntitle: {}\nversion: {}\ntype: {}\nsource: {}\ndefault: {}\nselected: {}\nreported: {}\n",
                    row.id,
                    row.show_title
                        .clone()
                        .or_else(|| row.title.clone())
                        .unwrap_or_default(),
                    row.version.clone().unwrap_or_default(),
                    row.kind.clone().unwrap_or_default(),
                    row.source.clone().unwrap_or_default(),
                    row.is_default,
                    row.is_selected,
                    row.is_reported,
                );
                let mut extra: Vec<_> = row.extra.iter().collect();
                extra.sort_by(|a, b| a.0.cmp(b.0));
                for (k, v) in extra {
                    let v = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    text.push_str(&format!("{k}: {v}\n"));
                }
                ctx.push(TextView::boxed(format!("boot entry {}", row.id), &text));
            }
            Action::Enable => ctx.exec(Exec::new("bootctl", &["set-default", &row.id]), true),
            Action::Start => ctx.exec(Exec::new("bootctl", &["set-oneshot", &row.id]), true),
            _ => {}
        }
    }
}
