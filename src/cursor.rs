//! Cursor control - equivalent to rcurses Cursor module

use std::io::{self, Write};

pub struct Cursor;

impl Cursor {
    /// Set cursor position (col, row) - 1-indexed like rcurses
    pub fn set(col: u16, row: u16) {
        print!("\x1b[{};{}H", row, col);
        io::stdout().flush().ok();
    }

    /// Move to specific row
    pub fn row(r: u16) {
        let (_, c) = Self::pos();
        Self::set(c, r);
    }

    /// Move to specific column
    pub fn col(c: u16) {
        let (r, _) = Self::pos();
        Self::set(c, r);
    }

    /// Query cursor position - returns (row, col)
    pub fn pos() -> (u16, u16) {
        // Use crossterm's position query
        crossterm::cursor::position().unwrap_or((0, 0))
    }

    /// Move up n rows
    pub fn up(n: u16) {
        print!("\x1b[{}A", n);
        io::stdout().flush().ok();
    }

    /// Move down n rows
    pub fn down(n: u16) {
        print!("\x1b[{}B", n);
        io::stdout().flush().ok();
    }

    /// Move right n columns
    pub fn right(n: u16) {
        print!("\x1b[{}C", n);
        io::stdout().flush().ok();
    }

    /// Move left n columns
    pub fn left(n: u16) {
        print!("\x1b[{}D", n);
        io::stdout().flush().ok();
    }

    /// Move to start of next line
    pub fn next_line() {
        print!("\x1b[E");
        io::stdout().flush().ok();
    }

    /// Move to start of previous line
    pub fn prev_line() {
        print!("\x1b[F");
        io::stdout().flush().ok();
    }

    /// Save cursor position
    pub fn save() {
        print!("\x1b7");
        io::stdout().flush().ok();
    }

    /// Restore saved cursor position
    pub fn restore() {
        print!("\x1b8");
        io::stdout().flush().ok();
    }

    /// Hide cursor
    pub fn hide() {
        crossterm::execute!(io::stdout(), crossterm::cursor::Hide).ok();
    }

    /// Show cursor
    pub fn show() {
        crossterm::execute!(io::stdout(), crossterm::cursor::Show).ok();
    }

    /// Set the host terminal's caret shape via DECSCUSR (`CSI N q`).
    /// Common values:
    ///   * 0 / 1 — blinking block (terminal default)
    ///   * 2     — steady block
    ///   * 3     — blinking underline
    ///   * 4     — steady underline
    ///   * 5     — blinking bar
    ///   * 6     — steady bar
    /// Used by editors that change cursor shape per mode (e.g. block in
    /// Normal, bar in Insert).
    pub fn shape(n: u8) {
        print!("\x1b[{} q", n);
        io::stdout().flush().ok();
    }

    /// Clear n characters from cursor position
    pub fn clear_char(n: u16) {
        print!("\x1b[{}X", n);
        io::stdout().flush().ok();
    }

    /// Clear entire line, cursor to start
    pub fn clear_line() {
        print!("\x1b[2K\r");
        io::stdout().flush().ok();
    }

    /// Clear from start of line to cursor
    pub fn clear_line_before() {
        print!("\x1b[1K");
        io::stdout().flush().ok();
    }

    /// Clear from cursor to end of line
    pub fn clear_line_after() {
        print!("\x1b[K");
        io::stdout().flush().ok();
    }

    /// Clear from cursor to bottom of screen
    pub fn clear_screen_down() {
        print!("\x1b[J");
        io::stdout().flush().ok();
    }

    /// Scroll terminal up one line
    pub fn scroll_up() {
        print!("\x1bM");
        io::stdout().flush().ok();
    }

    /// Scroll terminal down one line
    pub fn scroll_down() {
        print!("\x1bD");
        io::stdout().flush().ok();
    }
}
