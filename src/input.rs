//! Input handling - equivalent to rcurses Input module (getchr)

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

pub struct Input;

impl Input {
    /// Read a single key event, returning a named string like rcurses.
    /// Returns None on timeout (if timeout_secs is Some).
    pub fn getchr(timeout_secs: Option<u64>) -> Option<String> {
        let available = if let Some(secs) = timeout_secs {
            event::poll(Duration::from_secs(secs)).unwrap_or(false)
        } else {
            event::poll(Duration::from_secs(86400)).unwrap_or(false)
        };

        if !available {
            return None;
        }

        let ev = match event::read() {
            Ok(ev) => ev,
            Err(_) => return None,
        };

        match ev {
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                Some(Self::key_to_string(code, modifiers))
            }
            Event::Resize(_, _) => Some("RESIZE".to_string()),
            // Bracketed paste — apps that called EnableBracketedPaste receive
            // the whole pasted payload as one event. Encode as "PASTE\x00<text>"
            // so apps can detect it via .starts_with("PASTE\x00").
            Event::Paste(s) => Some(format!("PASTE\x00{}", s)),
            _ => None,
        }
    }

    /// Convert a crossterm KeyEvent to rcurses-compatible string
    fn key_to_string(code: KeyCode, mods: KeyModifiers) -> String {
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);

        match code {
            KeyCode::Esc => "ESC".to_string(),
            KeyCode::Enter => "ENTER".to_string(),
            KeyCode::Tab => {
                if shift { "S-TAB".to_string() } else { "TAB".to_string() }
            }
            KeyCode::BackTab => "S-TAB".to_string(),
            KeyCode::Backspace => {
                // Modifier on Backspace requires the terminal to send
                // distinct sequences (kitty keyboard protocol or
                // xterm modifyOtherKeys=2). Apps that want S-BACK
                // must enable crossterm's keyboard enhancement on
                // startup; without it, Shift+Backspace looks
                // identical to plain Backspace at the byte level.
                if ctrl { "WBACK".to_string() }
                else if shift { "S-BACK".to_string() }
                else { "BACK".to_string() }
            }
            KeyCode::Delete => {
                if ctrl { "C-DEL".to_string() } else { "DEL".to_string() }
            }
            KeyCode::Insert => {
                if ctrl { "C-INS".to_string() } else { "INS".to_string() }
            }
            KeyCode::Up => {
                if ctrl { "C-UP".to_string() }
                else if shift { "S-UP".to_string() }
                else { "UP".to_string() }
            }
            KeyCode::Down => {
                if ctrl { "C-DOWN".to_string() }
                else if shift { "S-DOWN".to_string() }
                else { "DOWN".to_string() }
            }
            KeyCode::Left => {
                if ctrl { "C-LEFT".to_string() }
                else if shift { "S-LEFT".to_string() }
                else { "LEFT".to_string() }
            }
            KeyCode::Right => {
                if ctrl { "C-RIGHT".to_string() }
                else if shift { "S-RIGHT".to_string() }
                else { "RIGHT".to_string() }
            }
            KeyCode::Home => {
                if ctrl { "C-HOME".to_string() } else { "HOME".to_string() }
            }
            KeyCode::End => {
                if ctrl { "C-END".to_string() } else { "END".to_string() }
            }
            KeyCode::PageUp => {
                if ctrl { "C-PgUP".to_string() } else { "PgUP".to_string() }
            }
            KeyCode::PageDown => {
                if ctrl { "C-PgDOWN".to_string() } else { "PgDOWN".to_string() }
            }
            KeyCode::F(n) => format!("F{}", n),
            KeyCode::Char(' ') if ctrl => "C-SPACE".to_string(),
            KeyCode::Char(c) if ctrl => {
                format!("C-{}", c.to_ascii_uppercase())
            }
            KeyCode::Char(c) => c.to_string(),
            _ => String::new(),
        }
    }
}
