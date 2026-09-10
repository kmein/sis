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
use crate::{
    event::{Effect, Exec},
    keys::{self, Action, Binding, Key},
};

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

/// A plain text view with a title, optionally (re)loaded from a command.
pub struct TextView {
    title: String,
    pane: TextPane,
    source: Option<Exec>,
    id: u64,
    loaded: bool,
}

impl TextView {
    pub fn new(title: impl Into<String>, text: &str) -> Self {
        let mut pane = TextPane::default();
        pane.set_lines(text.lines().map(|l| Line::raw(l.to_owned())).collect());
        Self {
            title: title.into(),
            pane,
            source: None,
            id: next_id(),
            loaded: true,
        }
    }

    pub fn boxed(title: impl Into<String>, text: &str) -> Box<dyn View> {
        Box::new(Self::new(title, text))
    }

    /// A view showing a command's output; `ctrl-r` runs it again.
    pub fn command(title: impl Into<String>, exec: Exec) -> Box<dyn View> {
        Box::new(Self {
            title: title.into(),
            pane: TextPane::default(),
            source: Some(exec),
            id: next_id(),
            loaded: false,
        })
    }

    fn reload(&mut self, ctx: &mut Ctx<'_>) {
        if let Some(exec) = &self.source {
            let mut exec = exec.clone();
            exec.title = None;
            exec.deliver = Some(self.id);
            ctx.effect(Effect::Exec(exec));
        }
    }
}

fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

impl View for TextView {
    fn title(&self) -> String {
        self.title.clone()
    }

    fn bindings(&self) -> &'static [Binding] {
        BINDINGS
    }

    fn on_enter(&mut self, ctx: &mut Ctx<'_>) {
        self.reload(ctx);
    }

    fn on_data(&mut self, _kind: crate::store::DataKind, _ctx: &mut Ctx<'_>) {}

    fn on_tick(&mut self, _ctx: &mut Ctx<'_>) {}

    fn on_exec(&mut self, id: u64, output: &Result<String, String>) {
        if id != self.id {
            return;
        }
        self.loaded = true;
        let lines = match output {
            Ok(text) => text.lines().map(|l| Line::raw(l.to_owned())).collect(),
            Err(err) => err.lines().map(|l| Line::raw(l.to_owned())).collect(),
        };
        self.pane.set_lines(lines);
    }

    fn on_key(&mut self, key: &KeyEvent, _ctx: &mut Ctx<'_>) -> Handled {
        match keys::lookup(BINDINGS, key) {
            Some(b) => self.pane.on_global(b.action),
            None => Handled::No,
        }
    }

    fn on_global(&mut self, action: Action, ctx: &mut Ctx<'_>) -> Handled {
        if action == Action::Refresh && self.source.is_some() {
            self.reload(ctx);
            return Handled::Yes;
        }
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
