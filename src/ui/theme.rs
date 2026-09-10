//! Colours and styles.

use ratatui::style::{Color, Modifier, Style};

use crate::systemd::types::{ActiveState, LoadState};

#[derive(Debug, Clone)]
pub struct Theme {
    pub border: Style,
    pub title: Style,
    pub header: Style,
    pub selected: Style,
    pub name: Style,
    pub dim: Style,
    pub job: Style,
    pub key: Style,
    pub label: Style,
    pub logo: Style,
    pub info_key: Style,
    pub info_value: Style,
    pub ok: Style,
    pub warn: Style,
    pub error: Style,
    pub prompt: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border: Style::default().fg(Color::Cyan),
            title: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            header: Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            selected: Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD),
            name: Style::default().fg(Color::Cyan),
            dim: Style::default().fg(Color::DarkGray),
            job: Style::default().fg(Color::Magenta),
            key: Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
            label: Style::default().fg(Color::Gray),
            logo: Style::default().fg(Color::Magenta),
            info_key: Style::default().fg(Color::Yellow),
            info_value: Style::default().fg(Color::White),
            ok: Style::default().fg(Color::Green),
            warn: Style::default().fg(Color::Yellow),
            error: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            prompt: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        }
    }
}

impl Theme {
    pub fn active(&self, state: &ActiveState) -> Style {
        match state {
            ActiveState::Active => self.ok,
            ActiveState::Failed => self.error,
            ActiveState::Activating | ActiveState::Deactivating | ActiveState::Reloading | ActiveState::Refreshing => {
                self.warn
            }
            ActiveState::Inactive => self.dim,
            ActiveState::Maintenance | ActiveState::Other(_) => self.warn,
        }
    }

    pub fn load(&self, state: &LoadState) -> Style {
        match state {
            LoadState::Loaded => Style::default(),
            LoadState::Masked => self.warn,
            LoadState::NotFound | LoadState::BadSetting | LoadState::Error => self.error,
            _ => self.dim,
        }
    }

    pub fn file_state(&self, state: Option<&str>) -> Style {
        match state {
            Some("enabled" | "enabled-runtime" | "alias" | "static" | "indirect" | "generated") => Style::default(),
            Some("disabled") => self.dim,
            Some("masked" | "masked-runtime" | "bad") => self.warn,
            _ => self.dim,
        }
    }

    pub fn priority(&self, prio: u8) -> Style {
        match prio {
            0 ..= 2 => self.error,
            3 => Style::default().fg(Color::Red),
            4 => self.warn,
            5 => Style::default().add_modifier(Modifier::BOLD),
            7 => self.dim,
            _ => Style::default(),
        }
    }
}
