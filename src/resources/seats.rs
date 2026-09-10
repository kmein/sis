//! `:seats` — `loginctl list-seats`.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, widgets::Cell};

use super::{Column, Ctx, Resource, Settings, SortKey};
use crate::{
    event::Exec,
    keys::{Action, Binding, Key},
    store::{DataKind, SeatRow, Store},
    systemd::fetch::FetchKind,
    ui::theme::Theme,
};

pub struct SeatsResource;

const COLUMNS: &[Column] = &[Column::new("SEAT", Constraint::Fill(1))];

const BINDINGS: &[Binding] = &[Binding::new(
    Key::code(KeyCode::Enter),
    Action::Select,
    "Status",
)];

impl Resource for SeatsResource {
    type Row = SeatRow;

    const ALIASES: &'static [&'static str] = &["seat"];
    const NAME: &'static str = "seats";

    fn columns() -> &'static [Column] {
        COLUMNS
    }

    fn fetches() -> &'static [(FetchKind, Duration)] {
        const FETCHES: &[(FetchKind, Duration)] = &[(FetchKind::Seats, Duration::from_secs(10))];
        FETCHES
    }

    fn affected_by(kind: DataKind) -> bool {
        kind == DataKind::Seats
    }

    fn rows(store: &Store, _settings: &Settings) -> Vec<SeatRow> {
        store.seats.clone()
    }

    fn key(row: &SeatRow) -> &str {
        &row.seat
    }

    fn cells(r: &SeatRow, theme: &Theme) -> Vec<Cell<'static>> {
        vec![Cell::from(r.seat.clone()).style(theme.name)]
    }

    fn sort_key(r: &SeatRow, _col: usize) -> SortKey {
        SortKey::Str(r.seat.clone())
    }

    fn sort_column(action: Action) -> Option<usize> {
        (action == Action::SortName).then_some(0)
    }

    fn matches(r: &SeatRow, needle: &str) -> bool {
        r.seat.contains(needle)
    }

    fn bindings() -> &'static [Binding] {
        BINDINGS
    }

    fn on_action(row: &SeatRow, action: Action, ctx: &mut Ctx<'_>) {
        if action == Action::Select {
            ctx.exec(
                Exec::new("loginctl", &["seat-status", "--no-pager", &row.seat])
                    .show(format!("seat {}", row.seat)),
                false,
            );
        }
    }
}
