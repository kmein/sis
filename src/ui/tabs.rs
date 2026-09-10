//! The view strip: every root view, the current one highlighted.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::{app::App, resources::CYCLE};

pub fn draw(f: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.theme;
    let current = app.root();
    let items: Vec<(String, bool)> = CYCLE
        .iter()
        .map(|n| (format!(" {n} "), *n == current))
        .collect();
    let width = area.width as usize;
    let total: usize = items.iter().map(|(t, _)| t.width() + 1).sum();

    // Keep the current view visible on narrow terminals by dropping leading items.
    let mut start = 0;
    if total > width {
        let pos = items.iter().position(|(_, cur)| *cur).unwrap_or(0);
        let mut used: usize = items[pos..]
            .iter()
            .map(|(t, _)| t.width() + 1)
            .take(1)
            .sum();
        start = pos;
        while start > 0 && used + items[start - 1].0.width() < width {
            start -= 1;
            used += items[start].0.width() + 1;
        }
    }

    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled("… ", theme.dim));
    }
    for (text, cur) in &items[start..] {
        spans.push(Span::styled(
            text.clone(),
            if *cur { theme.selected } else { theme.label },
        ));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
