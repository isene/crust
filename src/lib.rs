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
pub mod text;

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

    /// Announce this app's identity to the host terminal so the X11 window's
    /// `WM_NAME` and `WM_ICON_NAME` reflect the running app (not the
    /// terminal binary). Window managers that match by `WM_NAME` —
    /// e.g. CHasm's per-class assignment table — can then route Fe₂O₃
    /// TUIs to the right workspace.
    ///
    /// Emits:
    /// - `OSC 0 ; <name> ST` — sets icon name **and** window title.
    /// - `OSC 1 ; <name> ST` — sets icon name only (some terminals).
    /// - `OSC 2 ; <name> ST` — sets window title only.
    ///
    /// All three because terminals split responsibility differently; the host
    /// terminal updates `WM_NAME` from whichever it sees most recently.
    ///
    /// (When the glass terminal grows a custom OSC for `WM_CLASS`, this
    /// helper will be extended to emit it too.)
    pub fn set_app_identity(name: &str) {
        print!("\x1b]0;{}\x07", name);
        print!("\x1b]1;{}\x07", name);
        print!("\x1b]2;{}\x07", name);
        io::stdout().flush().ok();
    }

    /// Best-effort: ask the terminal for the kitty keyboard
    /// disambiguation flag so apps can distinguish modified keys
    /// (e.g. Shift+Backspace from plain Backspace, Ctrl+Tab from
    /// plain Tab). Terminals that don't grok the CSI 'u' protocol
    /// silently ignore the request and we fall back to legacy
    /// single-byte keycodes. Safe to call after `init()`.
    pub fn enable_modifier_keys() {
        use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
        let _ = crossterm::execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
        );
    }

    /// Companion to `enable_modifier_keys`; call before `cleanup()`
    /// so the terminal returns to legacy keyboard mode for whatever
    /// runs next in the same session.
    pub fn disable_modifier_keys() {
        use crossterm::event::PopKeyboardEnhancementFlags;
        let _ = crossterm::execute!(io::stdout(), PopKeyboardEnhancementFlags);
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

/// Per-character cell width matching glass's actual rendering rules.
///
/// unicode-width's defaults disagree with glass on a handful of cases
/// because glass widens specific BMP codepoints to handle their
/// VS16-emoji-presentation form (e.g. ⚠ U+26A0 → 2 cells), and forces
/// every non-BMP routed codepoint to wide (e.g. regional indicator
/// 🇺🇸 = 2+2 = 4 cells, where unicode-width would give 1+1 = 2).
///
/// When crust's wrap/truncate/pad logic uses unicode-width directly,
/// any mismatch with glass causes a line to render one or more cells
/// wider than crust expected — overflowing into the right border or
/// the next pane and leaving rendering gaps after a redraw. This
/// function brings the counting back in sync.
pub fn cell_width(c: char) -> usize {
    let cp = c as u32;
    // All non-BMP codepoints render 2 cells in glass (anything routed
    // to put_emoji is forced wide there). Matches all common emoji
    // ranges (1F300+, 1F600+, 1F680+, 1F900+, 1FA00+) and notably
    // regional indicators (1F1E6..1F1FF) which unicode-width gives 1.
    if cp > 0xFFFF {
        return 2;
    }
    // BMP emoji ranges that glass routes through put_emoji + is_wide_bmp:
    //   2300..23FF, 2600..27BF, 2B00..2BFF
    // Inside those, glass returns 2 if codepoint is in wide_bmp_ranges,
    // else 1. Outside those, glass uses the text path (1 cell for
    // anything narrow, ambiguous, or neutral).
    let in_emoji_range = matches!(
        cp,
        0x2300..=0x23FF | 0x2600..=0x27BF | 0x2B00..=0x2BFF
    );
    if in_emoji_range {
        return if is_glass_wide_bmp(cp) { 2 } else { 1 };
    }
    use unicode_width::UnicodeWidthChar;
    UnicodeWidthChar::width(c).unwrap_or(0)
}

/// Mirror of glass.asm's wide_bmp_ranges table, restricted to the
/// codepoints that actually fall inside one of glass's emoji-routing
/// ranges (entries outside those ranges are dead code in glass and
/// would mismatch — for those, fall through to unicode-width).
fn is_glass_wide_bmp(cp: u32) -> bool {
    matches!(cp,
        // 2300..23FF: Misc Technical
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 |
        // 2600..27BF: Misc Symbols + Dingbats
        0x2614..=0x2615 | 0x2648..=0x2653 | 0x267F | 0x2693 |
        0x26A0 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE |
        0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA |
        0x26F2..=0x26F3 | 0x26F5 | 0x26FA | 0x26FD |
        0x2705 | 0x270A..=0x270B | 0x2728 | 0x274C | 0x274E |
        0x2753..=0x2755 | 0x2757 | 0x2795..=0x2797 |
        0x27B0 | 0x27BF |
        // 2B00..2BFF: Misc Symbols and Arrows
        0x2B05..=0x2B07 | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55
    )
}

/// Calculate visible display width of a string (excluding ANSI, handling Unicode)
pub fn display_width(s: &str) -> usize {
    let stripped = strip_ansi(s);
    stripped.chars().map(cell_width).sum()
}

/// Truncate a string to max_width visible characters, preserving ANSI codes.
/// Appends an ellipsis "…" marker when actual truncation happens so the
/// reader can see the line was cut. If the input already fits in max_width,
/// returns it unchanged (plus a color reset). Like rcurses' shorten method.
pub fn truncate_ansi(s: &str, max_width: usize) -> String {
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
    // Track whether an OSC 8 hyperlink is currently open so we can close
    // it before the SGR reset — otherwise the hyperlink state leaks past
    // this line and kitty's url_style underline bleeds into every cell
    // that follows (including unrelated panes).
    let mut osc8_open = false;
    let mut osc_accum = String::new();

    for ch in s.chars() {
        if in_osc {
            result.push(ch);
            if ch == '\x07' || osc_saw_esc {
                // OSC terminated. If body starts with `8;`, update OSC 8
                // open/closed state from the URL field.
                if let Some(rest) = osc_accum.strip_prefix("8;") {
                    if let Some(sep) = rest.find(';') {
                        let url = &rest[sep + 1..];
                        osc8_open = !url.is_empty();
                    }
                }
                in_osc = false;
                osc_saw_esc = false;
                osc_accum.clear();
            } else if ch == '\x1b' {
                osc_saw_esc = true;
            } else {
                osc_accum.push(ch);
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
                osc_accum.clear();
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
        let w = cell_width(ch);
        if visible + w > target {
            break;
        }
        visible += w;
        result.push(ch);
    }
    if max_width >= 2 {
        result.push('\u{2026}'); // ellipsis
    }
    // If truncation cut a line while an OSC 8 hyperlink was still open,
    // close it explicitly — \x1b[0m does NOT close OSC 8 state, and
    // leaving it open makes kitty underline every cell that follows.
    if osc8_open {
        result.push_str("\x1b]8;;\x1b\\");
    }
    // Close with reset to prevent color bleeding
    result.push_str("\x1b[0m");
    result
}
