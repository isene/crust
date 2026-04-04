//! Text styling utilities - equivalent to rcurses String extensions
//!
//! Provides ANSI color and attribute formatting for strings.

/// Apply foreground color (0-255)
pub fn fg(text: &str, color: u8) -> String {
    format!("\x1b[38;5;{}m{}\x1b[0m", color, text)
}

/// Apply foreground color from RGB hex string
pub fn fg_rgb(text: &str, hex: &str) -> String {
    if let Some((r, g, b)) = parse_hex(hex) {
        format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text)
    } else {
        text.to_string()
    }
}

/// Apply background color (0-255)
pub fn bg(text: &str, color: u8) -> String {
    format!("\x1b[48;5;{}m{}\x1b[0m", color, text)
}

/// Apply background color from RGB hex string
pub fn bg_rgb(text: &str, hex: &str) -> String {
    if let Some((r, g, b)) = parse_hex(hex) {
        format!("\x1b[48;2;{};{};{}m{}\x1b[0m", r, g, b, text)
    } else {
        text.to_string()
    }
}

/// Apply both foreground and background (0-255)
pub fn fb(text: &str, fgc: u8, bgc: u8) -> String {
    format!("\x1b[38;5;{};48;5;{}m{}\x1b[0m", fgc, bgc, text)
}

/// Bold
pub fn bold(text: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", text)
}

/// Italic
pub fn italic(text: &str) -> String {
    format!("\x1b[3m{}\x1b[0m", text)
}

/// Underline
pub fn underline(text: &str) -> String {
    format!("\x1b[4m{}\x1b[0m", text)
}

/// Blink
pub fn blink(text: &str) -> String {
    format!("\x1b[5m{}\x1b[0m", text)
}

/// Reverse video
pub fn reverse(text: &str) -> String {
    format!("\x1b[7m{}\x1b[0m", text)
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
