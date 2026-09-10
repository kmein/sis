//! `systemctl status` and `systemctl show` for one unit, in tabs.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::{Ctx, Handled, View, journal::JournalView, text::TextPane, units};
use crate::{
    keys::{self, Action, Binding, Key},
    store::{DataKind, UnitDetail},
    systemd::{
        fetch::{FetchKind, prop_str, prop_u32, prop_u64},
        journal::JournalSpec,
        types::{ActiveState, LoadState, Process, UnitKind},
        watch::SystemdSignal,
    },
    ui::{format, theme::Theme},
};

const REFRESH: Duration = Duration::from_secs(2);
const SIGNAL_THROTTLE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    Status,
    Properties,
    CGroup,
    File,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Status, Tab::Properties, Tab::CGroup, Tab::File];

    fn label(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Properties => "properties",
            Self::CGroup => "cgroup",
            Self::File => "file",
        }
    }

    fn next(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

const BINDINGS: &[Binding] = &[
    Binding::new(Key::code(KeyCode::Tab), Action::TabNext, "Next tab"),
    Binding::new(Key::ch('1'), Action::Tab1, "Status").quiet(),
    Binding::new(Key::ch('2'), Action::Tab2, "Properties").quiet(),
    Binding::new(Key::ch('3'), Action::Tab3, "CGroup").quiet(),
    Binding::new(Key::ch('4'), Action::Tab4, "File").quiet(),
    Binding::new(Key::ch('w'), Action::ToggleWrap, "Wrap").quiet(),
    Binding::new(Key::ch('l'), Action::Logs, "Logs"),
    Binding::new(Key::ch('s'), Action::Start, "Start"),
    Binding::new(Key::ch('x'), Action::Stop, "Stop").confirm(),
    Binding::new(Key::ch('r'), Action::Restart, "Restart").confirm(),
    Binding::new(Key::ch('R'), Action::Reload, "Reload"),
    Binding::new(Key::ch('e'), Action::Enable, "Enable"),
    Binding::new(Key::ch('d'), Action::Disable, "Disable").confirm(),
    Binding::new(Key::ch('m'), Action::Mask, "Mask").confirm(),
    Binding::new(Key::ch('M'), Action::Unmask, "Unmask"),
    Binding::new(Key::ch('f'), Action::ResetFailed, "Reset failed"),
    Binding::new(Key::ctrl('k'), Action::Kill, "Kill").confirm(),
    Binding::new(Key::ch('!'), Action::Shell, "Unit shell"),
    Binding::new(Key::ctrl('g'), Action::Debug, "Unit gdb").quiet(),
    Binding::new(Key::ch('C'), Action::CriticalChain, "Critical chain"),
    Binding::new(Key::ch('V'), Action::Verify, "Verify").quiet(),
    Binding::new(Key::ch('Y'), Action::Dump, "Dump state").quiet(),
];

pub struct UnitDetailView {
    name: String,
    path: Option<OwnedObjectPath>,
    tab: Tab,
    panes: HashMap<Tab, TextPane>,
    last_fetch: Option<Instant>,
    has_data: bool,
}

impl UnitDetailView {
    pub fn new(name: &str, path: Option<OwnedObjectPath>, tab: Tab) -> Self {
        Self {
            name: name.to_owned(),
            path,
            tab,
            panes: Tab::ALL.iter().map(|t| (*t, TextPane::default())).collect(),
            last_fetch: None,
            has_data: false,
        }
    }

    pub fn boxed(name: &str, path: Option<OwnedObjectPath>, tab: Tab) -> Box<dyn View> {
        Box::new(Self::new(name, path, tab))
    }

    fn fetch(&mut self, ctx: &mut Ctx<'_>) {
        ctx.fetch(FetchKind::Detail {
            name: self.name.clone(),
            path: self.path.clone(),
        });
        self.last_fetch = Some(ctx.now);
    }

    fn pane(&mut self) -> &mut TextPane {
        self.panes.get_mut(&self.tab).expect("every tab has a pane")
    }

    fn rebuild(&mut self, detail: &UnitDetail, theme: &Theme) {
        self.has_data = true;
        self.path = Some(detail.path.clone());
        let status = status_lines(detail, theme);
        let props = property_lines(detail, theme);
        let cgroup = cgroup_lines(detail, theme);
        let file = file_lines(detail, theme);
        for (tab, lines) in [
            (Tab::Status, status),
            (Tab::Properties, props),
            (Tab::CGroup, cgroup),
            (Tab::File, file),
        ] {
            self.panes.get_mut(&tab).expect("pane").set_lines(lines);
        }
    }
}

impl View for UnitDetailView {
    fn title(&self) -> String {
        self.name.clone()
    }

    fn bindings(&self) -> &'static [Binding] {
        BINDINGS
    }

    fn on_enter(&mut self, ctx: &mut Ctx<'_>) {
        if let Some(detail) = ctx.store.detail.get(&self.name) {
            let theme = ctx.theme;
            self.rebuild(detail, theme);
        }
        self.fetch(ctx);
    }

    fn on_data(&mut self, kind: DataKind, ctx: &mut Ctx<'_>) {
        if kind == DataKind::UnitDetail
            && let Some(detail) = ctx.store.detail.get(&self.name)
        {
            let theme = ctx.theme;
            self.rebuild(detail, theme);
        }
    }

    fn on_tick(&mut self, ctx: &mut Ctx<'_>) {
        if self
            .last_fetch
            .is_none_or(|t| ctx.now.duration_since(t) >= REFRESH)
        {
            self.fetch(ctx);
        }
    }

    fn on_signal(&mut self, signal: &SystemdSignal, ctx: &mut Ctx<'_>) {
        let mine = match signal {
            SystemdSignal::UnitsDirty(paths) => {
                self.path.as_ref().is_some_and(|p| paths.contains(p))
            }
            SystemdSignal::UnitNew(n) | SystemdSignal::UnitRemoved(n) => *n == self.name,
            SystemdSignal::JobNew { unit, .. } | SystemdSignal::JobRemoved { unit, .. } => {
                *unit == self.name
            }
            SystemdSignal::Reloading(_) => false,
        };
        if mine
            && self
                .last_fetch
                .is_none_or(|t| ctx.now.duration_since(t) >= SIGNAL_THROTTLE)
        {
            self.fetch(ctx);
        }
    }

    fn on_key(&mut self, key: &KeyEvent, ctx: &mut Ctx<'_>) -> Handled {
        let Some(binding) = keys::lookup(BINDINGS, key) else {
            return Handled::No;
        };
        match binding.action {
            Action::TabNext => self.tab = self.tab.next(),
            Action::Tab1 => self.tab = Tab::Status,
            Action::Tab2 => self.tab = Tab::Properties,
            Action::Tab3 => self.tab = Tab::CGroup,
            Action::Tab4 => self.tab = Tab::File,
            Action::ToggleWrap => {
                self.pane().on_global(Action::ToggleWrap);
            }
            Action::Logs => ctx.push(JournalView::boxed(JournalSpec::unit(ctx.scope, &self.name))),
            other if units::analysis_action(&self.name, other, ctx) => {}
            action => units::perform(&self.name, action, binding.confirm, ctx),
        }
        Handled::Yes
    }

    fn on_global(&mut self, action: Action, _ctx: &mut Ctx<'_>) -> Handled {
        self.pane().on_global(action)
    }

    fn set_filter(&mut self, needle: &str) {
        let pane = self.pane();
        pane.filter = needle.to_owned();
        pane.scroll = 0;
    }

    fn filter(&self) -> &str {
        &self.panes[&self.tab].filter
    }

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, ctx: &Ctx<'_>) {
        let theme = ctx.theme;
        let mut title = vec![Span::raw(" ")];
        for tab in Tab::ALL {
            let style = if tab == self.tab {
                theme.selected
            } else {
                theme.dim
            };
            title.push(Span::styled(format!(" {} ", tab.label()), style));
            title.push(Span::raw(" "));
        }
        let pane = self.panes.get_mut(&self.tab).expect("pane");
        let pos = pane.position();
        if !pane.filter.is_empty() {
            title.push(Span::styled(format!("/{} ", pane.filter), theme.title));
        }
        title.push(Span::styled(format!("[{pos}] "), theme.dim));
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border)
            .title(Line::from(title))
            .title_bottom(Line::styled(format!(" {} ", self.name), theme.title));
        if !self.has_data {
            pane.set_lines(vec![Line::styled("loading…", theme.dim)]);
        }
        pane.render(f, area, block);
    }
}

fn kv(theme: &Theme, key: &str, value: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:>12}: "), theme.info_key),
        Span::styled(value.into(), style),
    ])
}

fn kv_plain(theme: &Theme, key: &str, value: impl Into<String>) -> Line<'static> {
    kv(theme, key, value, Style::default())
}

/// Continuation line under a key.
fn cont(value: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::raw(" ".repeat(14)),
        Span::styled(value.into(), style),
    ])
}

fn str_list(props: &HashMap<String, OwnedValue>, name: &str) -> Vec<String> {
    props
        .get(name)
        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        .unwrap_or_default()
}

/// The `systemctl status` tab.
fn status_lines(d: &UnitDetail, theme: &Theme) -> Vec<Line<'static>> {
    let u = &d.unit;
    let t = &d.typed;
    let active = ActiveState::parse(&prop_str(u, "ActiveState").unwrap_or_default());
    let load = LoadState::parse(&prop_str(u, "LoadState").unwrap_or_default());
    let sub = prop_str(u, "SubState").unwrap_or_default();
    let dot_style = theme.active(&active);
    let dot = match active {
        ActiveState::Active => "●",
        ActiveState::Failed => "×",
        ActiveState::Activating | ActiveState::Deactivating | ActiveState::Reloading => "↻",
        _ => "○",
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{dot} "), dot_style),
        Span::styled(d.name.clone(), theme.name),
        Span::styled(
            format!(" - {}", prop_str(u, "Description").unwrap_or_default()),
            theme.dim,
        ),
    ])];

    let fragment = prop_str(u, "FragmentPath").unwrap_or_default();
    let file_state = prop_str(u, "UnitFileState").unwrap_or_default();
    let preset = prop_str(u, "UnitFilePreset").unwrap_or_default();
    let mut loaded = load.to_string();
    let mut parts = Vec::new();
    if !fragment.is_empty() {
        parts.push(fragment);
    }
    if !file_state.is_empty() {
        parts.push(file_state);
    }
    if !preset.is_empty() {
        parts.push(format!("preset: {preset}"));
    }
    if !parts.is_empty() {
        loaded = format!("{loaded} ({})", parts.join("; "));
    }
    lines.push(kv(theme, "Loaded", loaded, theme.load(&load)));
    let drop_ins = str_list(u, "DropInPaths");
    if !drop_ins.is_empty() {
        lines.push(kv_plain(theme, "Drop-In", drop_ins[0].clone()));
        for p in &drop_ins[1..] {
            lines.push(cont(p.clone(), Style::default()));
        }
    }

    let since = if active == ActiveState::Active {
        prop_u64(u, "ActiveEnterTimestamp").unwrap_or(0)
    } else {
        prop_u64(u, "StateChangeTimestamp").unwrap_or(0)
    };
    let mut active_text = format!("{active} ({sub})");
    if since != 0 {
        active_text = format!(
            "{active_text} since {}; {} ago",
            format::timestamp(since),
            format::age(since)
        );
    }
    lines.push(kv(theme, "Active", active_text, theme.active(&active)));
    if let Some(result) = prop_str(t, "Result").filter(|r| r != "success") {
        lines.push(kv(theme, "Result", result, theme.error));
    }
    let triggered_by = str_list(u, "TriggeredBy");
    if !triggered_by.is_empty() {
        lines.push(kv_plain(theme, "TriggeredBy", triggered_by.join(" ")));
    }
    let triggers = str_list(u, "Triggers");
    if !triggers.is_empty() {
        lines.push(kv_plain(theme, "Triggers", triggers.join(" ")));
    }
    let docs = str_list(u, "Documentation");
    if !docs.is_empty() {
        lines.push(kv(theme, "Docs", docs[0].clone(), theme.dim));
        for doc in &docs[1..] {
            lines.push(cont(doc.clone(), theme.dim));
        }
    }
    if let Some(v) = u.get("InvocationID") {
        let id = format::value(v);
        if !id.is_empty() && id.chars().any(|c| c != '0') {
            lines.push(kv(theme, "Invocation", id, theme.dim));
        }
    }

    match UnitKind::of(&d.name) {
        UnitKind::Service => {
            if let Some(pid) = prop_u32(t, "MainPID").filter(|p| *p != 0) {
                let comm = d
                    .processes
                    .iter()
                    .find(|p| p.1 == pid)
                    .map(|p| p.2.clone())
                    .unwrap_or_default();
                let comm = comm
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_owned();
                lines.push(kv_plain(theme, "Main PID", format!("{pid} ({comm})")));
            }
            if let Some(status) = prop_str(t, "StatusText").filter(|s| !s.is_empty()) {
                lines.push(kv_plain(theme, "Status", format!("\"{status}\"")));
            }
            if let Some(n) = prop_u32(t, "NRestarts").filter(|n| *n > 0) {
                lines.push(kv(theme, "Restarts", n.to_string(), theme.warn));
            }
        }
        UnitKind::Timer => {
            if let Some(next) = prop_u64(t, "NextElapseUSecRealtime").filter(|n| *n != 0) {
                lines.push(kv_plain(
                    theme,
                    "Trigger",
                    format!("{}; in {}", format::timestamp(next), format::until(next)),
                ));
            }
            if let Some(last) = prop_u64(t, "LastTriggerUSec").filter(|n| *n != 0) {
                lines.push(kv_plain(
                    theme,
                    "Last",
                    format!("{}; {} ago", format::timestamp(last), format::age(last)),
                ));
            }
        }
        UnitKind::Socket => {
            if let Some(v) = t.get("Listen")
                && let Ok(listen) = Vec::<(String, String)>::try_from(v.clone())
            {
                for (i, (kind, addr)) in listen.iter().enumerate() {
                    let text = format!("{addr} ({kind})");
                    lines.push(if i == 0 {
                        kv_plain(theme, "Listen", text)
                    } else {
                        cont(text, Style::default())
                    });
                }
            }
        }
        UnitKind::Mount | UnitKind::Automount | UnitKind::Swap => {
            if let Some(what) = prop_str(t, "What").filter(|s| !s.is_empty()) {
                lines.push(kv_plain(theme, "What", what));
            }
            if let Some(where_) = prop_str(t, "Where").filter(|s| !s.is_empty()) {
                lines.push(kv_plain(theme, "Where", where_));
            }
        }
        _ => {}
    }

    if let Some(tasks) = prop_u64(t, "TasksCurrent") {
        let limit = prop_u64(t, "TasksMax")
            .map(|m| format!(" (limit: {m})"))
            .unwrap_or_default();
        lines.push(kv_plain(theme, "Tasks", format!("{tasks}{limit}")));
    }
    if let Some(mem) = prop_u64(t, "MemoryCurrent") {
        let mut extra = Vec::new();
        if let Some(peak) = prop_u64(t, "MemoryPeak") {
            extra.push(format!("peak: {}", format::bytes(peak)));
        }
        if let Some(swap) = prop_u64(t, "MemorySwapCurrent").filter(|s| *s > 0) {
            extra.push(format!("swap: {}", format::bytes(swap)));
        }
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!(" ({})", extra.join(", "))
        };
        lines.push(kv_plain(
            theme,
            "Memory",
            format!("{}{extra}", format::bytes(mem)),
        ));
    }
    if let Some(cpu) = prop_u64(t, "CPUUsageNSec") {
        lines.push(kv_plain(theme, "CPU", format::cpu_ns(cpu)));
    }
    if let (Some(r), Some(w)) = (prop_u64(t, "IOReadBytes"), prop_u64(t, "IOWriteBytes")) {
        lines.push(kv_plain(
            theme,
            "IO",
            format!("{} read, {} written", format::bytes(r), format::bytes(w)),
        ));
    }
    if let Some(cg) = prop_str(t, "ControlGroup").filter(|s| !s.is_empty()) {
        lines.push(kv_plain(theme, "CGroup", cg.clone()));
        lines.extend(process_tree(&cg, &d.processes, theme, 14));
    }
    lines
}

/// `systemd-cgls`-like tree of the unit's processes.
fn process_tree(
    root: &str,
    processes: &[Process],
    theme: &Theme,
    indent: usize,
) -> Vec<Line<'static>> {
    let mut procs: Vec<&Process> = processes.iter().collect();
    procs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut lines = Vec::new();
    let mut current_group = String::new();
    let pad = " ".repeat(indent);
    let count = procs.len();
    for (i, p) in procs.iter().enumerate() {
        let group =
            p.0.strip_prefix(root)
                .unwrap_or(&p.0)
                .trim_start_matches('/')
                .to_owned();
        let last = i + 1 == count;
        let mut depth = 0;
        if !group.is_empty() {
            depth = 1;
            if group != current_group {
                current_group = group.clone();
                lines.push(Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(
                        format!("{}{group}", if last { "└─" } else { "├─" }),
                        theme.dim,
                    ),
                ]));
            }
        }
        let branch = if last { "└─" } else { "├─" };
        let sub = if depth > 0 { "│ " } else { "" };
        lines.push(Line::from(vec![
            Span::raw(format!("{pad}{sub}{branch}")),
            Span::styled(p.1.to_string(), theme.name),
            Span::raw(format!(" {}", p.2)),
        ]));
    }
    lines
}

fn property_lines(d: &UnitDetail, theme: &Theme) -> Vec<Line<'static>> {
    let mut all: Vec<(&String, &OwnedValue)> = d.unit.iter().chain(d.typed.iter()).collect();
    all.sort_by(|a, b| a.0.cmp(b.0));
    all.into_iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k}="), theme.info_key),
                Span::raw(format::value(v)),
            ])
        })
        .collect()
}

fn cgroup_lines(d: &UnitDetail, theme: &Theme) -> Vec<Line<'static>> {
    let cg = prop_str(&d.typed, "ControlGroup").unwrap_or_default();
    if d.processes.is_empty() {
        return vec![Line::styled(
            if cg.is_empty() {
                "no control group"
            } else {
                "no processes"
            },
            theme.dim,
        )];
    }
    let mut lines = vec![Line::styled(cg.clone(), theme.title)];
    lines.extend(process_tree(&cg, &d.processes, theme, 0));
    lines
}

fn file_lines(d: &UnitDetail, theme: &Theme) -> Vec<Line<'static>> {
    if d.files.is_empty() {
        return vec![Line::styled(
            "no unit file (transient or generated in memory)",
            theme.dim,
        )];
    }
    let mut lines = Vec::new();
    for (path, contents) in &d.files {
        lines.push(Line::styled(format!("# {path}"), theme.title));
        match contents {
            Ok(text) => {
                for l in text.lines() {
                    let style = if l.starts_with('[') {
                        theme.info_key
                    } else if l.starts_with('#') || l.starts_with(';') {
                        theme.dim
                    } else {
                        Style::default()
                    };
                    lines.push(Line::styled(l.to_owned(), style));
                }
            }
            Err(err) => lines.push(Line::styled(format!("cannot read: {err}"), theme.error)),
        }
        lines.push(Line::raw(""));
    }
    lines
}
