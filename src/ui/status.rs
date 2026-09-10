//! The bottom line: prompt, flash message, or breadcrumbs.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{App, Prompt},
    event::Level,
};

pub fn draw(f: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.theme;
    let line = match &app.prompt {
        Prompt::Command(text) => Line::from(vec![
            Span::styled("🐚> ", theme.prompt),
            Span::raw(text.clone()),
            Span::styled("█", theme.prompt),
        ]),
        Prompt::Filter(text) => Line::from(vec![
            Span::styled("🔍> ", theme.prompt),
            Span::raw(text.clone()),
            Span::styled("█", theme.prompt),
        ]),
        Prompt::Confirm { text, .. } => Line::from(vec![
            Span::styled(format!("{text} [y/N] "), theme.warn),
            Span::styled("█", theme.prompt),
        ]),
        Prompt::None => match app.flash_message() {
            Some(status) => {
                let style = match status.level {
                    Level::Info => theme.info_value,
                    Level::Ok => theme.ok,
                    Level::Error => theme.error,
                };
                Line::from(Span::styled(status.text.clone(), style))
            }
            None => {
                let mut spans = Vec::new();
                for (i, title) in app.breadcrumbs().into_iter().enumerate() {
                    let style = if i == 0 { theme.selected } else { theme.title };
                    spans.push(Span::styled(format!(" <{title}> "), style));
                    spans.push(Span::raw(" "));
                }
                Line::from(spans)
            }
        },
    };
    f.render_widget(Paragraph::new(line), area);
}
