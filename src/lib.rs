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

    /// Set terminal window title via OSC escape
    pub fn set_title(title: &str) {
        print!("\x1b]0;{}\x07", title);
        io::stdout().flush().ok();
    }
}

/// Base64 encode bytes (used by OSC 52 clipboard, Kitty protocol, etc.)
pub fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[(n >> 18 & 63) as usize] as char);
        result.push(CHARS[(n >> 12 & 63) as usize] as char);
        if chunk.len() > 1 { result.push(CHARS[(n >> 6 & 63) as usize] as char); } else { result.push('='); }
        if chunk.len() > 2 { result.push(CHARS[(n & 63) as usize] as char); } else { result.push('='); }
    }
    result
}

/// Copy text to clipboard via OSC 52 escape sequence.
/// Works in wezterm, kitty, xterm, and other modern terminals.
/// Also tries xclip as a non-blocking fallback.
/// `selection`: "clipboard" (default) or "primary".
pub fn clipboard_copy(text: &str, selection: &str) {
    let sel_code = if selection == "primary" { "p" } else { "c" };
    let encoded = base64_encode(text.as_bytes());
    print!("\x1b]52;{};{}\x07", sel_code, encoded);
    io::stdout().flush().ok();

    // Also try xclip as backup (non-blocking spawn)
    let sel_arg = if selection == "primary" { "primary" } else { "clipboard" };
    if let Ok(mut child) = std::process::Command::new("xclip")
        .args(["-selection", sel_arg])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(ref mut stdin) = child.stdin {
            let _ = io::Write::write_all(stdin, text.as_bytes());
        }
        std::thread::spawn(move || { let _ = child.wait(); });
    }
}

/// Shell-escape a string (single-quote wrapping with quote escaping)
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Strip ANSI escape sequences from a string
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    let mut in_csi = false;
    let mut in_osc = false;
    let mut osc_saw_esc = false;
    for ch in s.chars() {
        if in_osc {
            // OSC terminated by BEL (\x07) or ST (\x1b\\)
            if ch == '\x07' {
                in_osc = false;
            } else if osc_saw_esc {
                // Any char after ESC ends the OSC (expected \\)
                in_osc = false;
                osc_saw_esc = false;
            } else if ch == '\x1b' {
                osc_saw_esc = true;
            }
            continue;
        }
        if in_escape {
            if ch == '[' {
                in_csi = true;
                in_escape = false;
            } else if ch == ']' {
                in_osc = true;
                in_escape = false;
            } else {
                in_escape = false;
                // Non-CSI/OSC escape: skip this char too
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
/// Appends an ellipsis "…" marker when actual truncation happens so the
/// reader can see the line was cut. If the input already fits in max_width,
/// returns it unchanged (plus a color reset). Like rcurses' shorten method.
pub fn truncate_ansi(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    // If the whole string already fits, no marker needed.
    if display_width(s) <= max_width {
        let mut out = s.to_string();
        out.push_str("\x1b[0m");
        return out;
    }

    // Reserve one visible column for the "…" marker, but only if the pane
    // is at least 2 cols wide. For 1-col panes we just hard-cut.
    let target = if max_width >= 2 { max_width - 1 } else { max_width };

    let mut result = String::new();
    let mut visible = 0;
    let mut in_escape = false;
    let mut in_csi = false;
    let mut in_osc = false;
    let mut osc_saw_esc = false;

    for ch in s.chars() {
        if in_osc {
            result.push(ch);
            if ch == '\x07' {
                in_osc = false;
            } else if osc_saw_esc {
                in_osc = false;
                osc_saw_esc = false;
            } else if ch == '\x1b' {
                osc_saw_esc = true;
            }
            continue;
        }
        if in_escape {
            result.push(ch);
            if ch == '[' {
                in_csi = true;
                in_escape = false;
            } else if ch == ']' {
                in_osc = true;
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
        if visible + w > target {
            break;
        }
        visible += w;
        result.push(ch);
    }
    if max_width >= 2 {
        result.push('\u{2026}'); // ellipsis
    }
    // Close with reset to prevent color bleeding
    result.push_str("\x1b[0m");
    result
}
