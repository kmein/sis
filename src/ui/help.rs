//! The `?` overlay listing every binding.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{app::App, keys};

pub fn draw(f: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.theme;
    let view_bindings = app.view().bindings();
    let row = |b: &crate::keys::Binding| {
        Line::from(vec![
            Span::styled(format!("  {:<10}", b.key.label()), theme.key),
            Span::styled(b.label, theme.label),
            Span::styled(if b.confirm { " *" } else { "" }, theme.warn),
        ])
    };

    let mut left = vec![Line::styled(format!("{} keys", app.view().title()), theme.title)];
    left.extend(view_bindings.iter().map(row));
    left.push(Line::raw(""));
    left.push(Line::styled("commands", theme.title));
    for name in crate::resources::names() {
        left.push(Line::from(vec![Span::styled("  :", theme.key), Span::styled(name, theme.label)]));
    }
    for (name, what) in [("user", "user manager"), ("system", "system manager"), ("all", "toggle inactive"), ("help", ""), ("quit", "")] {
        left.push(Line::from(vec![
            Span::styled(format!("  :{name:<9}"), theme.key),
            Span::styled(what, theme.label),
        ]));
    }
    left.push(Line::raw(""));
    left.push(Line::styled("* asks first", theme.warn));

    let mut right = vec![Line::styled("global keys", theme.title)];
    right.extend(keys::GLOBAL.iter().filter(|b| !view_bindings.iter().any(|v| v.key == b.key)).map(row));

    let height = (left.len().max(right.len()) as u16 + 2).min(area.height);
    let [popup] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
    let [popup] = Layout::horizontal([Constraint::Length(74)]).flex(Flex::Center).areas(popup);
    f.render_widget(Clear, popup);
    let block = Block::default().borders(Borders::ALL).border_style(theme.border).title(" help (any key closes) ");
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(inner);
    f.render_widget(Paragraph::new(left), l);
    f.render_widget(Paragraph::new(right), r);
}
