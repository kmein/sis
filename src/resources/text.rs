//! Scrollable, filterable text: the building block of the detail tabs and
//! of every "show me this command's output" view.

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{Ctx, Handled, View};
use crate::keys::{self, Action, Binding, Key};

/// The state shared by all text panes.
#[derive(Default)]
pub struct TextPane {
    pub lines: Vec<Line<'static>>,
    pub scroll: usize,
    pub filter: String,
    pub wrap: bool,
    last_height: usize,
}

impl TextPane {
    pub fn set_lines(&mut self, lines: Vec<Line<'static>>) {
        self.lines = lines;
        self.clamp();
    }

    /// Lines that match the filter (all of them when it is empty).
    fn visible(&self) -> Vec<Line<'static>> {
        if self.filter.is_empty() {
            return self.lines.clone();
        }
        let needle = self.filter.to_lowercase();
        self.lines
            .iter()
            .filter(|l| l.to_string().to_lowercase().contains(&needle))
            .cloned()
            .collect()
    }

    fn len(&self) -> usize {
        if self.filter.is_empty() {
            self.lines.len()
        } else {
            self.visible().len()
        }
    }

    fn clamp(&mut self) {
        let max = self.len().saturating_sub(self.last_height.max(1));
        self.scroll = self.scroll.min(max);
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = (self.scroll as isize + delta).max(0) as usize;
        self.clamp();
    }

    /// Handle the global navigation actions.
    pub fn on_global(&mut self, action: Action) -> Handled {
        let page = self.last_height.max(1) as isize;
        match action {
            Action::Up => self.scroll_by(-1),
            Action::Down => self.scroll_by(1),
            Action::PageUp => self.scroll_by(-page),
            Action::PageDown => self.scroll_by(page),
            Action::Top => self.scroll = 0,
            Action::Bottom => self.scroll_by(isize::MAX / 2),
            Action::ToggleWrap => self.wrap = !self.wrap,
            _ => return Handled::No,
        }
        Handled::Yes
    }

    pub fn render(&mut self, f: &mut Frame<'_>, area: Rect, block: Block<'static>) {
        let inner = block.inner(area);
        self.last_height = inner.height as usize;
        self.clamp();
        let lines = self.visible();
        let end = lines.len();
        let start = self.scroll.min(end);
        let mut paragraph = Paragraph::new(lines[start..end].to_vec()).block(block);
        if self.wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }
        f.render_widget(paragraph, area);
    }

    pub fn position(&self) -> String {
        let len = self.len();
        if len == 0 {
            String::new()
        } else {
            format!("{}/{len}", (self.scroll + 1).min(len))
        }
    }
}

const BINDINGS: &[Binding] = &[Binding::new(Key::ch('w'), Action::ToggleWrap, "Wrap")];

/// A plain text view with a title.
pub struct TextView {
    title: String,
    pane: TextPane,
}

impl TextView {
    pub fn new(title: impl Into<String>, text: &str) -> Self {
        let mut pane = TextPane::default();
        pane.set_lines(text.lines().map(|l| Line::raw(l.to_owned())).collect());
        Self {
            title: title.into(),
            pane,
        }
    }

    pub fn boxed(title: impl Into<String>, text: &str) -> Box<dyn View> {
        Box::new(Self::new(title, text))
    }
}

impl View for TextView {
    fn title(&self) -> String {
        self.title.clone()
    }

    fn bindings(&self) -> &'static [Binding] {
        BINDINGS
    }

    fn on_enter(&mut self, _ctx: &mut Ctx<'_>) {}

    fn on_data(&mut self, _kind: crate::store::DataKind, _ctx: &mut Ctx<'_>) {}

    fn on_tick(&mut self, _ctx: &mut Ctx<'_>) {}

    fn on_key(&mut self, key: &KeyEvent, _ctx: &mut Ctx<'_>) -> Handled {
        match keys::lookup(BINDINGS, key) {
            Some(b) => self.pane.on_global(b.action),
            None => Handled::No,
        }
    }

    fn on_global(&mut self, action: Action, _ctx: &mut Ctx<'_>) -> Handled {
        self.pane.on_global(action)
    }

    fn set_filter(&mut self, needle: &str) {
        self.pane.filter = needle.to_owned();
        self.pane.scroll = 0;
    }

    fn filter(&self) -> &str {
        &self.pane.filter
    }

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, ctx: &Ctx<'_>) {
        let title = format!(" {} [{}] ", self.title, self.pane.position());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(ctx.theme.border)
            .title(title);
        self.pane.render(f, area, block);
    }
}
