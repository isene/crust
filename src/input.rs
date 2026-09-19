//! Input handling - equivalent to rcurses Input module (getchr)

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;

/// What happened to a key, for readers that ask with `event_ms`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyState { Pressed, Repeated, Released }

pub struct Input;

impl Input {
    /// Non-blocking peek: returns true if at least one input event is
    /// already queued, false otherwise. Used by image-displaying TUIs
    /// to skip expensive previews while the user is still hammering
    /// j/k on autorepeat — render the cheap state now, do the heavy
    /// graphics work after the burst ends.
    pub fn peek_pending() -> bool {
        crossterm::event::poll(Duration::from_millis(0)).unwrap_or(false)
    }

    /// As [`getchr`](Self::getchr), with the wait in milliseconds. For a
    /// loop that is also draining something else, a stream of answer
    /// chunks say: block in the kernel until a key or the deadline, then
    /// look at the other thing. Idle programs keep using `getchr(None)`.
    pub fn getchr_ms(timeout_ms: u64) -> Option<String> {
        if !event::poll(Duration::from_millis(timeout_ms)).unwrap_or(false) { return None; }
        Self::getchr(Some(0))
    }

    /// As [`getchr_ms`](Self::getchr_ms), with what happened to the key:
    /// pressed, repeated or released. Releases and repeats only arrive
    /// when the terminal speaks the kitty keyboard protocol and
    /// [`Crust::enable_key_release`](crate::Crust::enable_key_release)
    /// was called; elsewhere every event is a press. A game needs this;
    /// `getchr` keeps skipping releases so no other app sees a key twice.
    pub fn event_ms(timeout_ms: u64) -> Option<(String, KeyState)> {
        if !event::poll(Duration::from_millis(timeout_ms)).unwrap_or(false) { return None; }
        match event::read() {
            Ok(Event::Key(KeyEvent { code, modifiers, kind, .. })) => {
                let state = match kind {
                    KeyEventKind::Release => KeyState::Released,
                    KeyEventKind::Repeat => KeyState::Repeated,
                    KeyEventKind::Press => KeyState::Pressed,
                };
                Some((Self::key_to_string(code, modifiers), state))
            }
            Ok(Event::Resize(_, _)) => Some(("RESIZE".to_string(), KeyState::Pressed)),
            _ => None,
        }
    }

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
            // A release is not a key press; only `event_ms` reports it.
            Event::Key(KeyEvent { kind: KeyEventKind::Release, .. }) => None,
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
