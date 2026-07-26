//! Text styling utilities - equivalent to rcurses String extensions
//!
//! Provides ANSI color and attribute formatting for strings.

/// Apply foreground color (0-255). Resets only fg (SGR 39), not all attributes.
pub fn fg(text: &str, color: u8) -> String {
    format!("\x1b[38;5;{}m{}\x1b[39m", color, text)
}

/// Wrap `text` so it renders in the terminal's native foreground —
/// useful for emoji glyphs that would otherwise be tinted by an
/// outer `fg(...)`. Emits `CSI 39` (default fg) before and after, so
/// the glyph reaches the terminal without any palette override and
/// glass's emoji-routing renders its colour-font form.
pub fn native(text: &str) -> String {
    format!("\x1b[39m{}\x1b[39m", text)
}

/// Apply foreground color from RGB hex string
pub fn fg_rgb(text: &str, hex: &str) -> String {
    if let Some((r, g, b)) = parse_hex(hex) {
        format!("\x1b[38;2;{};{};{}m{}\x1b[39m", r, g, b, text)
    } else {
        text.to_string()
    }
}

/// Apply background color (0-255). Resets only bg (SGR 49), not all attributes.
pub fn bg(text: &str, color: u8) -> String {
    format!("\x1b[48;5;{}m{}\x1b[49m", color, text)
}

/// Apply background color from RGB hex string
pub fn bg_rgb(text: &str, hex: &str) -> String {
    if let Some((r, g, b)) = parse_hex(hex) {
        format!("\x1b[48;2;{};{};{}m{}\x1b[49m", r, g, b, text)
    } else {
        text.to_string()
    }
}

/// Apply both foreground and background (0-255). Resets both fg+bg.
pub fn fb(text: &str, fgc: u8, bgc: u8) -> String {
    format!("\x1b[38;5;{};48;5;{}m{}\x1b[39;49m", fgc, bgc, text)
}

/// Bold. Resets only bold (SGR 22).
pub fn bold(text: &str) -> String {
    format!("\x1b[1m{}\x1b[22m", text)
}

/// Dim / faint. Resets only bold+faint (SGR 22), so surrounding color
/// survives. This is the single most common attribute in the suite's
/// label/value panels, and every app used to hand-roll it.
pub fn dim(text: &str) -> String {
    format!("\x1b[2m{}\x1b[22m", text)
}

/// Italic. Resets only italic (SGR 23).
pub fn italic(text: &str) -> String {
    format!("\x1b[3m{}\x1b[23m", text)
}

/// Underline. Resets only underline (SGR 24).
pub fn underline(text: &str) -> String {
    format!("\x1b[4m{}\x1b[24m", text)
}

/// Blink. Resets only blink (SGR 25).
pub fn blink(text: &str) -> String {
    format!("\x1b[5m{}\x1b[25m", text)
}

/// Reverse video. Resets only reverse (SGR 27).
pub fn reverse(text: &str) -> String {
    format!("\x1b[7m{}\x1b[27m", text)
}

/// Apply multiple style attributes in a single ANSI sequence (no nesting issues).
/// Pass None for fg/bg to leave them at terminal default.
/// attrs: combination of 'b' (bold), 'i' (italic), 'u' (underline), 'l' (blink), 'r' (reverse)
pub fn styled(text: &str, fgc: Option<u8>, bgc: Option<u8>, attrs: &str) -> String {
    let mut codes = Vec::new();
    if let Some(f) = fgc { codes.push(format!("38;5;{}", f)); }
    if let Some(b) = bgc { codes.push(format!("48;5;{}", b)); }
    for ch in attrs.chars() {
        match ch {
            'b' => codes.push("1".to_string()),
            'i' => codes.push("3".to_string()),
            'u' => codes.push("4".to_string()),
            'l' => codes.push("5".to_string()),
            'r' => codes.push("7".to_string()),
            _ => {}
        }
    }
    if codes.is_empty() {
        text.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

/// Coded format: "fg,bg,biulr" like rcurses .c() method
pub fn coded(text: &str, spec: &str) -> String {
    let parts: Vec<&str> = spec.split(',').collect();
    let mut codes = Vec::new();
    if let Some(fgc) = parts.first() {
        if let Ok(n) = fgc.parse::<u8>() {
            codes.push(format!("38;5;{}", n));
        }
    }
    if let Some(bgc) = parts.get(1) {
        if let Ok(n) = bgc.parse::<u8>() {
            codes.push(format!("48;5;{}", n));
        }
    }
    if let Some(attrs) = parts.get(2) {
        for ch in attrs.chars() {
            match ch {
                'b' => codes.push("1".to_string()),
                'i' => codes.push("3".to_string()),
                'u' => codes.push("4".to_string()),
                'l' => codes.push("5".to_string()),
                'r' => codes.push("7".to_string()),
                _ => {}
            }
        }
    }
    if codes.is_empty() {
        text.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

/// Truecolor (24-bit) sibling of `coded`: optional (r,g,b) fg/bg. Terminates
/// with a full reset (`\x1b[0m`) so a host pane restores its own colors after.
pub fn coded_rgb(text: &str, fg: Option<(u8, u8, u8)>, bg: Option<(u8, u8, u8)>) -> String {
    let mut codes = Vec::new();
    if let Some((r, g, b)) = fg { codes.push(format!("38;2;{};{};{}", r, g, b)); }
    if let Some((r, g, b)) = bg { codes.push(format!("48;2;{};{};{}", r, g, b)); }
    if codes.is_empty() {
        text.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

/// Truecolor with attributes, in ONE escape sequence.
///
/// The 24-bit counterpart of `styled`: optional (r,g,b) foreground and
/// background plus any of `b` bold, `d` dim, `i` italic, `u` underline,
/// `l` blink, `r` reverse. Terminates with a full reset so a host pane
/// restores its own colors afterwards.
///
/// Use this instead of writing `\x1b[1;38;2;…m` by hand: nesting
/// `bold(coded_rgb(…))` works but emits two sequences per span, and the
/// inner reset silently cancels the outer attribute on some terminals.
pub fn rgb(text: &str, fg: Option<(u8, u8, u8)>, bg: Option<(u8, u8, u8)>, attrs: &str) -> String {
    let mut codes = Vec::new();
    for ch in attrs.chars() {
        match ch {
            'b' => codes.push("1".to_string()),
            'd' => codes.push("2".to_string()),
            'i' => codes.push("3".to_string()),
            'u' => codes.push("4".to_string()),
            'l' => codes.push("5".to_string()),
            'r' => codes.push("7".to_string()),
            _ => {}
        }
    }
    if let Some((r, g, b)) = fg {
        codes.push(format!("38;2;{r};{g};{b}"));
    }
    if let Some((r, g, b)) = bg {
        codes.push(format!("48;2;{r};{g};{b}"));
    }
    if codes.is_empty() {
        text.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

/// A bare SGR reset, for callers assembling their own spans.
pub const RESET: &str = "\x1b[0m";

/// An OSC 8 hyperlink: `label` becomes clickable, pointing at `url`.
///
/// Wrap the label yourself for styling — `hyperlink(url, &underline(text))`
/// — so this stays one concern. The link is closed with an empty OSC 8,
/// which is what stops terminals underlining the rest of the line.
///
/// Note for multi-stage renderers: only ONE stage may emit OSC 8. A later
/// URL-matching pass will otherwise find the URL *inside* this escape and
/// nest a second link, which kitty and glass resolve by eating the rest
/// of the line.
pub fn hyperlink(url: &str, label: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
}

/// Wrap an SGR parameter string that came from OUTSIDE the program —
/// `LS_COLORS` entries like `38;5;12` or `01;34`, a theme file, a
/// server response — into a usable escape. For colors the program picks
/// itself, use the typed helpers above instead.
pub fn sgr(spec: &str) -> String {
    format!("\x1b[{spec}m")
}

/// Just the "switch foreground to this 256-color" sequence, with no
/// reset. For inline switches inside a longer styled run, where a reset
/// would drop the background or attributes the caller set around it.
pub fn set_fg(color: u8) -> String {
    format!("\x1b[38;5;{color}m")
}

/// Truecolor sibling of `set_fg`.
pub fn set_fg_rgb(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

/// Background counterpart of `set_fg`.
pub fn set_bg(color: u8) -> String {
    format!("\x1b[48;5;{color}m")
}

/// Truecolor sibling of `set_bg`.
pub fn set_bg_rgb(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{r};{g};{b}m")
}

/// Return the background to the terminal default (SGR 49).
pub fn reset_bg() -> String {
    "\x1b[49m".to_string()
}

/// Return the foreground to the terminal default (SGR 39), leaving
/// background and attributes alone.
pub fn reset_fg() -> String {
    "\x1b[39m".to_string()
}

/// Parse hex color string ("#RRGGBB" or "#RGB") to (r, g, b)
pub fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

// Internal alias for backward compat within this module
fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> { parse_hex_color(hex) }

/// Convert RGB values to nearest xterm-256 color index
pub fn rgb_to_xterm(r: u8, g: u8, b: u8) -> u8 {
    // Grayscale ramp (indices 232-255)
    if r == g && g == b {
        if r < 8 { return 16; }
        if r > 248 { return 231; }
        return (((r as u16 - 8) * 24 / 247) as u8) + 232;
    }
    // 6x6x6 color cube (indices 16-231)
    16 + 36 * (r / 51) + 6 * (g / 51) + (b / 51)
}
