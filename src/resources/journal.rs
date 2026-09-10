//! A live journal tail for one unit (or pid, machine, everything).

use std::collections::VecDeque;

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::UnicodeWidthChar;

use super::{Ctx, Handled, View};
use crate::{
    event::Effect,
    keys::{self, Action, Binding, Key},
    store::DataKind,
    systemd::journal::{JournalEntry, JournalId, JournalItem, JournalSpec},
    ui::{format, theme::Theme},
};

const CAPACITY: usize = 10_000;

const BINDINGS: &[Binding] = &[
    Binding::new(Key::ch('f'), Action::ToggleFollow, "Follow"),
    Binding::new(Key::ch('w'), Action::ToggleWrap, "Wrap"),
    Binding::new(Key::ch('p'), Action::CyclePriority, "Priority"),
    Binding::new(Key::ctrl('l'), Action::Clear, "Clear"),
];

pub struct JournalView {
    id: JournalId,
    spec: JournalSpec,
    entries: VecDeque<JournalEntry>,
    /// Sequence number of `entries[0]`.
    first_seq: u64,
    /// Sequence numbers of entries passing the filters, ascending.
    shown: VecDeque<u64>,
    filter: String,
    /// Show only entries with priority <= this (None: everything).
    max_priority: Option<u8>,
    follow: bool,
    wrap: bool,
    /// Position in `shown` of the top line when not following.
    scroll: usize,
    ended: Option<Option<String>>,
    last_height: usize,
}

impl JournalView {
    pub fn new(spec: JournalSpec) -> Self {
        Self {
            id: JournalId::next(),
            spec,
            entries: VecDeque::new(),
            first_seq: 0,
            shown: VecDeque::new(),
            filter: String::new(),
            max_priority: None,
            follow: true,
            wrap: true,
            scroll: 0,
            ended: None,
            last_height: 1,
        }
    }

    pub fn boxed(spec: JournalSpec) -> Box<dyn View> {
        Box::new(Self::new(spec))
    }

    fn passes(&self, e: &JournalEntry) -> bool {
        self.max_priority.is_none_or(|p| e.priority <= p)
            && (self.filter.is_empty() || {
                let needle = self.filter.to_lowercase();
                e.message.to_lowercase().contains(&needle)
                    || e.identifier.to_lowercase().contains(&needle)
            })
    }

    fn push(&mut self, entry: JournalEntry) {
        let seq = self.first_seq + self.entries.len() as u64;
        if self.passes(&entry) {
            self.shown.push_back(seq);
        }
        self.entries.push_back(entry);
        if self.entries.len() > CAPACITY {
            self.entries.pop_front();
            self.first_seq += 1;
            while self.shown.front().is_some_and(|&s| s < self.first_seq) {
                self.shown.pop_front();
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
    }

    fn refilter(&mut self) {
        let first = self.first_seq;
        let entries = std::mem::take(&mut self.entries);
        self.shown = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.passes(e))
            .map(|(i, _)| first + i as u64)
            .collect();
        self.entries = entries;
        self.scroll = self.scroll.min(self.shown.len().saturating_sub(1));
    }

    fn entry(&self, seq: u64) -> Option<&JournalEntry> {
        self.entries.get((seq - self.first_seq) as usize)
    }

    fn scroll_by(&mut self, delta: isize) {
        if self.follow {
            // Leave follow mode at the bottom of the buffer.
            self.follow = false;
            self.scroll = self.shown.len().saturating_sub(self.last_height);
        }
        let max = self.shown.len().saturating_sub(1) as isize;
        self.scroll = (self.scroll as isize + delta).clamp(0, max.max(0)) as usize;
    }

    fn render_entry(&self, e: &JournalEntry, theme: &Theme, width: usize) -> Vec<Line<'static>> {
        let style = if e.is_from_systemd() {
            theme.name
        } else {
            theme.priority(e.priority)
        };
        let stamp = format::short_time(e.timestamp);
        let who = match e.pid {
            Some(pid) => format!("{}[{pid}]: ", e.identifier),
            None => format!("{}: ", e.identifier),
        };
        let text = format!("{stamp} {who}{}", e.message);
        if !self.wrap {
            return vec![Line::from(Span::styled(text, style))];
        }
        wrap(&text, width.max(10))
            .into_iter()
            .map(|l| Line::from(Span::styled(l, style)))
            .collect()
    }
}

/// Wrap on display width, continuing lines with a small indent.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current.push_str("  ");
            used = 2;
        }
        current.push(ch);
        used += w;
    }
    lines.push(current);
    lines
}

impl View for JournalView {
    fn title(&self) -> String {
        "logs".to_owned()
    }

    fn bindings(&self) -> &'static [Binding] {
        BINDINGS
    }

    fn on_enter(&mut self, ctx: &mut Ctx<'_>) {
        ctx.effect(Effect::OpenJournal {
            id: self.id,
            spec: self.spec.clone(),
        });
    }

    fn on_close(&mut self, ctx: &mut Ctx<'_>) {
        ctx.effect(Effect::CloseJournal(self.id));
    }

    fn on_data(&mut self, _kind: DataKind, _ctx: &mut Ctx<'_>) {}

    fn on_tick(&mut self, _ctx: &mut Ctx<'_>) {}

    fn on_journal(&mut self, id: JournalId, item: JournalItem) {
        if id != self.id {
            return;
        }
        match item {
            JournalItem::Entry(entry) => self.push(entry),
            JournalItem::Ended(error) => self.ended = Some(error),
        }
    }

    fn on_key(&mut self, key: &KeyEvent, _ctx: &mut Ctx<'_>) -> Handled {
        let Some(binding) = keys::lookup(BINDINGS, key) else {
            return Handled::No;
        };
        match binding.action {
            Action::ToggleFollow => {
                self.follow = !self.follow;
                if !self.follow {
                    self.scroll = self.shown.len().saturating_sub(self.last_height);
                }
            }
            Action::ToggleWrap => self.wrap = !self.wrap,
            Action::CyclePriority => {
                self.max_priority = match self.max_priority {
                    None => Some(4),
                    Some(4) => Some(3),
                    Some(_) => None,
                };
                self.refilter();
            }
            Action::Clear => {
                self.entries.clear();
                self.shown.clear();
                self.first_seq = 0;
                self.scroll = 0;
            }
            _ => return Handled::No,
        }
        Handled::Yes
    }

    fn on_global(&mut self, action: Action, _ctx: &mut Ctx<'_>) -> Handled {
        let page = self.last_height.max(1) as isize;
        match action {
            Action::Up => self.scroll_by(-1),
            Action::Down => self.scroll_by(1),
            Action::PageUp => self.scroll_by(-page),
            Action::PageDown => self.scroll_by(page),
            Action::Top => {
                self.follow = false;
                self.scroll = 0;
            }
            Action::Bottom => self.follow = true,
            _ => return Handled::No,
        }
        Handled::Yes
    }

    fn set_filter(&mut self, needle: &str) {
        self.filter = needle.to_owned();
        self.refilter();
    }

    fn filter(&self) -> &str {
        &self.filter
    }

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, ctx: &Ctx<'_>) {
        let theme = ctx.theme;
        let mut title = format!(" logs({}) ", self.spec.describe());
        if !self.filter.is_empty() {
            title.push_str(&format!("/{} ", self.filter));
        }
        if let Some(p) = self.max_priority {
            title.push_str(&format!("prio≤{p} "));
        }
        title.push_str(&format!(
            "[{}{}] ",
            self.shown.len(),
            if self.follow { " ⇣" } else { "" }
        ));
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border)
            .title(title);
        let inner = block.inner(area);
        let height = inner.height as usize;
        let width = inner.width as usize;
        self.last_height = height.max(1);

        let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);
        if self.follow {
            // Fill from the bottom up.
            let mut collected: Vec<Vec<Line<'static>>> = Vec::new();
            let mut total = 0;
            for &seq in self.shown.iter().rev() {
                let Some(e) = self.entry(seq) else { continue };
                let rendered = self.render_entry(e, theme, width);
                total += rendered.len();
                collected.push(rendered);
                if total >= height {
                    break;
                }
            }
            for rendered in collected.into_iter().rev() {
                lines.extend(rendered);
            }
            if lines.len() > height {
                lines.drain(..lines.len() - height);
            }
        } else {
            for &seq in self.shown.iter().skip(self.scroll) {
                let Some(e) = self.entry(seq) else { continue };
                lines.extend(self.render_entry(e, theme, width));
                if lines.len() >= height {
                    break;
                }
            }
            lines.truncate(height);
        }
        if let Some(ended) = &self.ended
            && lines.len() < height
        {
            lines.push(match ended {
                Some(err) => Line::styled(format!("journalctl: {err}"), theme.error),
                None => Line::styled("journalctl exited", theme.dim),
            });
        }
        if lines.is_empty() {
            let text = if self.entries.is_empty() {
                "waiting for journal…"
            } else {
                "no matching entries"
            };
            lines.push(Line::styled(text, theme.dim));
        }
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}
