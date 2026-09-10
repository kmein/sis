pub mod format;
pub mod header;
pub mod help;
pub mod status;
pub mod tabs;
pub mod theme;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
};

use crate::app::App;

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let [head, strip, body, foot] = Layout::vertical([
        Constraint::Length(header::HEIGHT),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    header::draw(f, head, app);
    tabs::draw(f, strip, app);
    app.render_view(f, body);
    status::draw(f, foot, app);
    if app.help {
        help::draw(f, body, app);
    }
}
