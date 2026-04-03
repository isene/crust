//! # crust - Rust TUI library
//!
//! A pane-based terminal UI library with ANSI colors, scrolling, input handling,
//! and diff-based rendering. Feature clone of rcurses (Ruby).
//!
//! ## Quick Start
//! ```no_run
//! use crust::{Crust, Pane, Input};
//!
//! fn main() {
//!     Crust::init();
//!     let (h, w) = Crust::terminal_size();
//!     let mut pane = Pane::new(1, 1, w, h - 1, 255, 0);
//!     pane.set_text("Hello from crust!");
//!     pane.refresh();
//!     let key = Input::getchr(None);
//!     Crust::cleanup();
//! }
//! ```

pub mod pane;
pub mod popup;
pub mod input;
pub mod cursor;
pub mod style;

pub use pane::Pane;
pub use popup::Popup;
pub use input::Input;
pub use cursor::Cursor;

use crossterm::terminal;
use std::io::{self, Write};

/// ANSI escape regex pattern (pre-compiled equivalent)
pub const ANSI_RE: &str = "\x1b\\[[0-9;]*m";

/// Initialize crust (alternate screen, raw mode, hide cursor)
pub struct Crust;

impl Crust {
    pub fn init() {
        let mut stdout = io::stdout();
        terminal::enable_raw_mode().ok();
        // Alternate screen buffer
        crossterm::execute!(stdout, terminal::EnterAlternateScreen).ok();
        // Hide cursor
        crossterm::execute!(stdout, crossterm::cursor::Hide).ok();
        // Disable line wrap to prevent artifacts
        print!("\x1b[?7l");
        stdout.flush().ok();
    }

    pub fn cleanup() {
        let mut stdout = io::stdout();
        // Re-enable line wrap
        print!("\x1b[?7h");
        // Show cursor
        crossterm::execute!(stdout, crossterm::cursor::Show).ok();
        // Leave alternate screen
        crossterm::execute!(stdout, terminal::LeaveAlternateScreen).ok();
        terminal::disable_raw_mode().ok();
        stdout.flush().ok();
    }

    pub fn terminal_size() -> (u16, u16) {
        terminal::size().unwrap_or((24, 80))
    }

    pub fn clear_screen() {
        print!("\x1b[2J\x1b[H");
        io::stdout().flush().ok();
    }
}

/// Strip ANSI escape sequences from a string
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    let mut in_csi = false;
    for ch in s.chars() {
        if in_escape {
            if ch == '[' {
                in_csi = true;
                in_escape = false;
            } else {
                in_escape = false;
                // Non-CSI escape: skip this char too
            }
            continue;
        }
        if in_csi {
            if ch.is_ascii_alphabetic() {
                in_csi = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        result.push(ch);
    }
    result
}

/// Calculate visible display width of a string (excluding ANSI, handling Unicode)
pub fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    let stripped = strip_ansi(s);
    UnicodeWidthStr::width(stripped.as_str())
}

/// Truncate a string to max_width visible characters, preserving ANSI codes.
/// Like rcurses' shorten method.
pub fn truncate_ansi(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut result = String::new();
    let mut visible = 0;
    let mut in_escape = false;
    let mut in_csi = false;

    for ch in s.chars() {
        if in_escape {
            result.push(ch);
            if ch == '[' {
                in_csi = true;
                in_escape = false;
            } else {
                in_escape = false;
            }
            continue;
        }
        if in_csi {
            result.push(ch);
            if ch.is_ascii_alphabetic() {
                in_csi = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_escape = true;
            result.push(ch);
            continue;
        }
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if visible + w > max_width {
            break;
        }
        visible += w;
        result.push(ch);
    }
    // Close with reset to prevent color bleeding
    result.push_str("\x1b[0m");
    result
}
