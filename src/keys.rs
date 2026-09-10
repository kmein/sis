//! Key bindings: a flat table of `key → action` with a label, so the header
//! hints and the help overlay are generated from the same source.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Global
    Quit,
    Back,
    Help,
    Command,
    Filter,
    Refresh,
    ToggleScope,
    ToggleAll,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    SortName,
    SortActive,
    SortType,
    SortLoad,
    SortMemory,
    KindService,
    KindTimer,
    KindSocket,
    KindTarget,
    KindMount,
    KindAll,
    /// Synthetic: settings changed, rebuild rows.
    SettingsChanged,
    TabNext,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    ToggleWrap,
    ToggleFollow,
    CyclePriority,
    Clear,
    // Row actions
    Select,
    Logs,
    Start,
    Stop,
    Restart,
    Reload,
    Enable,
    Disable,
    Mask,
    Unmask,
    ResetFailed,
    Cat,
    Kill,
    DaemonReload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Key {
    pub const fn ch(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: KeyModifiers::NONE,
        }
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: KeyModifiers::CONTROL,
        }
    }

    pub const fn code(code: KeyCode) -> Self {
        Self {
            code,
            mods: KeyModifiers::NONE,
        }
    }

    pub fn matches(&self, ev: &KeyEvent) -> bool {
        let mods = ev.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT);
        match (self.code, ev.code) {
            // Uppercase letters arrive with SHIFT set; ignore that bit.
            (KeyCode::Char(a), KeyCode::Char(b)) => a == b && mods == self.mods,
            (a, b) => a == b && mods == self.mods,
        }
    }

    /// Render like k9s: `<s>`, `<ctrl-a>`, `<enter>`.
    pub fn label(&self) -> String {
        let name = match self.code {
            KeyCode::Char(' ') => "space".to_owned(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".to_owned(),
            KeyCode::Esc => "esc".to_owned(),
            KeyCode::Tab => "tab".to_owned(),
            KeyCode::Backspace => "bksp".to_owned(),
            KeyCode::Up => "↑".to_owned(),
            KeyCode::Down => "↓".to_owned(),
            KeyCode::PageUp => "pgup".to_owned(),
            KeyCode::PageDown => "pgdn".to_owned(),
            KeyCode::Home => "home".to_owned(),
            KeyCode::End => "end".to_owned(),
            other => format!("{other:?}").to_lowercase(),
        };
        if self.mods.contains(KeyModifiers::CONTROL) {
            format!("<ctrl-{name}>")
        } else if self.mods.contains(KeyModifiers::ALT) {
            format!("<alt-{name}>")
        } else {
            format!("<{name}>")
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub key: Key,
    pub action: Action,
    pub label: &'static str,
    /// Ask before running.
    pub confirm: bool,
    /// Show in the header hint block (the help overlay shows everything).
    pub hint: bool,
}

impl Binding {
    pub const fn new(key: Key, action: Action, label: &'static str) -> Self {
        Self {
            key,
            action,
            label,
            confirm: false,
            hint: true,
        }
    }

    pub const fn confirm(self) -> Self {
        Self {
            confirm: true,
            ..self
        }
    }

    pub const fn quiet(self) -> Self {
        Self {
            hint: false,
            ..self
        }
    }
}

/// Bindings that apply in every view unless the view claims the key first.
pub const GLOBAL: &[Binding] = &[
    Binding::new(Key::ch(':'), Action::Command, "Command"),
    Binding::new(Key::ch('/'), Action::Filter, "Filter"),
    Binding::new(Key::ch('?'), Action::Help, "Help"),
    Binding::new(Key::code(KeyCode::Esc), Action::Back, "Back"),
    Binding::new(Key::ch('q'), Action::Quit, "Quit").quiet(),
    Binding::new(Key::ctrl('c'), Action::Quit, "Quit"),
    Binding::new(Key::ctrl('r'), Action::Refresh, "Refresh"),
    Binding::new(Key::ch('u'), Action::ToggleScope, "System/User"),
    Binding::new(Key::ctrl('a'), Action::ToggleAll, "Show all"),
    Binding::new(Key::ch('j'), Action::Down, "Down").quiet(),
    Binding::new(Key::code(KeyCode::Down), Action::Down, "Down").quiet(),
    Binding::new(Key::ch('k'), Action::Up, "Up").quiet(),
    Binding::new(Key::code(KeyCode::Up), Action::Up, "Up").quiet(),
    Binding::new(Key::ctrl('d'), Action::PageDown, "Page down").quiet(),
    Binding::new(Key::code(KeyCode::PageDown), Action::PageDown, "Page down").quiet(),
    Binding::new(Key::ctrl('u'), Action::PageUp, "Page up").quiet(),
    Binding::new(Key::code(KeyCode::PageUp), Action::PageUp, "Page up").quiet(),
    Binding::new(Key::ch('g'), Action::Top, "Top").quiet(),
    Binding::new(Key::code(KeyCode::Home), Action::Top, "Top").quiet(),
    Binding::new(Key::ch('G'), Action::Bottom, "Bottom").quiet(),
    Binding::new(Key::code(KeyCode::End), Action::Bottom, "Bottom").quiet(),
    Binding::new(Key::ch('N'), Action::SortName, "Sort name").quiet(),
    Binding::new(Key::ch('A'), Action::SortActive, "Sort active").quiet(),
    Binding::new(Key::ch('T'), Action::SortType, "Sort type").quiet(),
    Binding::new(Key::ch('L'), Action::SortLoad, "Sort load").quiet(),
    Binding::new(Key::ch('M'), Action::SortMemory, "Sort memory").quiet(),
    Binding::new(Key::ch('1'), Action::KindService, "Services").quiet(),
    Binding::new(Key::ch('2'), Action::KindTimer, "Timers").quiet(),
    Binding::new(Key::ch('3'), Action::KindSocket, "Sockets").quiet(),
    Binding::new(Key::ch('4'), Action::KindTarget, "Targets").quiet(),
    Binding::new(Key::ch('5'), Action::KindMount, "Mounts").quiet(),
    Binding::new(Key::ch('0'), Action::KindAll, "All kinds").quiet(),
];

/// Look a key event up in a binding table.
pub fn lookup<'a>(bindings: &'a [Binding], ev: &KeyEvent) -> Option<&'a Binding> {
    bindings.iter().find(|b| b.key.matches(ev))
}
