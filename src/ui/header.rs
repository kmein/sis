//! The k9s-style header: manager facts, key hints, logo.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{app::App, keys};

pub const LOGO: [&str; 5] = [
    r"     _     ",
    r" ___(_)___ ",
    r"/ __| / __|",
    r"\__ \ \__ \",
    r"|___/_|___/",
];

pub const HEIGHT: u16 = LOGO.len() as u16 + 1;

pub fn draw(f: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = &app.theme;
    let [info, hints, logo] =
        Layout::horizontal([Constraint::Length(32), Constraint::Fill(1), Constraint::Length(12)]).areas(area);

    let m = &app.store.manager;
    let kv = |k: &str, v: String| {
        Line::from(vec![Span::styled(format!("{k:<10}"), theme.info_key), Span::styled(v, theme.info_value)])
    };
    let scope = if app.connecting { format!("{} (connecting…)", app.scope) } else { app.scope.to_string() };
    let kinds = match &app.settings.kind {
        Some(kind) => format!("{kind}s only"),
        None => "all".to_owned(),
    };
    let lines = vec![
        kv("Manager:", scope),
        kv("State:", m.state.clone()),
        kv("Version:", m.version.clone()),
        kv("Units:", format!("{} loaded, {} failed", m.n_names, m.n_failed)),
        kv("Jobs:", m.n_jobs.to_string()),
        kv("Showing:", format!("{}, {kinds}", if app.settings.show_all { "all" } else { "active" })),
    ];
    f.render_widget(Paragraph::new(lines), info);

    // Hints: the view's bindings first, then the global ones, in columns.
    let mut items: Vec<(String, &str)> = app
        .view()
        .bindings()
        .iter()
        .chain(keys::GLOBAL.iter())
        .filter(|b| b.hint)
        .map(|b| (b.key.label(), b.label))
        .collect();
    items.dedup_by(|a, b| a.0 == b.0);
    let rows = area.height as usize;
    let cols = items.len().div_ceil(rows.max(1)).max(1);
    let widths: Vec<Constraint> = (0 .. cols).map(|_| Constraint::Length(25)).collect();
    let col_areas = Layout::horizontal(widths).split(hints);
    for (c, col_area) in col_areas.iter().enumerate() {
        let lines: Vec<Line> = items
            .iter()
            .skip(c * rows)
            .take(rows)
            .map(|(k, l)| Line::from(vec![Span::styled(format!("{k:<9}"), theme.key), Span::styled(*l, theme.label)]))
            .collect();
        f.render_widget(Paragraph::new(lines), *col_area);
    }

    let logo_lines: Vec<Line> = LOGO.iter().map(|l| Line::styled(*l, theme.logo)).collect();
    f.render_widget(Paragraph::new(logo_lines), logo);
}
