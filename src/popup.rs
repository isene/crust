//! Popup - modal dialog widget, equivalent to rcurses Popup
//!
//! A centered (or positioned) pane that overlays content with keyboard navigation.

use crate::{style, Input, Pane};

pub struct Popup {
    pub pane: Pane,
}

/// One row drawn as the selection bar. Reverse video is re-armed after
/// every reset inside the row, so an item carrying its own colors does
/// not drop the bar half way across.
fn select_bar(line: &str, width: usize) -> String {
    let body = line.replace(style::RESET, &format!("{}{}", style::RESET, style::REVERSE));
    let pad = width.saturating_sub(crate::display_width(line));
    format!("{}{body}{}{}", style::REVERSE, " ".repeat(pad), style::RESET)
}

impl Popup {
    /// Create a centered popup with given dimensions
    pub fn centered(w: u16, h: u16, fg: u16, bg: u16) -> Self {
        let (cols, rows) = crate::Crust::terminal_size();
        let x = (cols.saturating_sub(w)) / 2;
        let y = (rows.saturating_sub(h)) / 2;
        let mut pane = Pane::new(x.max(1), y.max(1), w, h, fg, bg);
        pane.border = true;
        pane.scroll = true;
        Self { pane }
    }

    /// Create a popup at specific position
    pub fn new(x: u16, y: u16, w: u16, h: u16, fg: u16, bg: u16) -> Self {
        let mut pane = Pane::new(x, y, w, h, fg, bg);
        pane.border = true;
        pane.scroll = true;
        Self { pane }
    }

    /// Show the popup as a read-only VIEWER: Up/Down (and k/j) scroll the
    /// content one line at a time, PgUp/PgDn/SPACE page, g/G jump to the
    /// edges. Any of ESC / q / ENTER closes. Use this for help screens and
    /// long texts — `modal` moves a selection index instead, which looks
    /// inert on non-menu content until the cursor walks off-screen.
    pub fn view(&mut self, content: &str) {
        self.pane.set_text(content);
        self.pane.ix = 0;
        self.pane.border_refresh();
        self.pane.refresh();
        loop {
            if let Some(key) = Input::getchr(None) {
                match key.as_str() {
                    "ESC" | "q" | "ENTER" => return,
                    "UP" | "k" => { self.pane.lineup(); }
                    "DOWN" | "j" => { self.pane.linedown(); }
                    "PgDOWN" | " " => self.pane.pagedown(),
                    "PgUP" | "b" => self.pane.pageup(),
                    "HOME" | "g" => self.pane.top(),
                    "END" | "G" => self.pane.bottom(),
                    _ => {}
                }
            }
        }
    }

    /// Show the popup as a MENU: one line per item, the current one drawn
    /// as a selection bar that Up/Down (and k/j) moves. ENTER returns its
    /// index, ESC or q returns None. Set `pane.index` before calling to
    /// open on a given item. For read-only text use `view` instead — a
    /// selection bar on a help screen is noise.
    pub fn modal(&mut self, content: &str) -> Option<usize> {
        let items: Vec<String> = content.split('\n').map(str::to_string).collect();
        let last = items.len().saturating_sub(1);
        // One item per row: a wrapped item would break the 1:1 mapping
        // between what the bar sits on and what ENTER returns.
        self.pane.wrap = false;
        self.pane.index = self.pane.index.min(last);
        self.pane.border_refresh();
        self.render(&items);

        loop {
            let key = match Input::getchr(None) {
                Some(k) => k,
                None => continue,
            };
            let page = (self.pane.h as usize).max(1);
            match key.as_str() {
                "ESC" | "q" => return None,
                "ENTER" => return Some(self.pane.index),
                "UP" | "k" => self.pane.index = self.pane.index.saturating_sub(1),
                "DOWN" | "j" => self.pane.index = (self.pane.index + 1).min(last),
                "PgUP" | "b" => self.pane.index = self.pane.index.saturating_sub(page),
                "PgDOWN" | " " => self.pane.index = (self.pane.index + page).min(last),
                "HOME" | "g" => self.pane.index = 0,
                "END" | "G" => self.pane.index = last,
                _ => continue,
            }
            self.render(&items);
        }
    }

    /// Draw the items with the current one highlighted, scrolling only as
    /// far as it takes to keep the selection on screen.
    fn render(&mut self, items: &[String]) {
        let h = (self.pane.h as usize).max(1);
        if self.pane.index < self.pane.ix {
            self.pane.ix = self.pane.index;
        } else if self.pane.index >= self.pane.ix + h {
            self.pane.ix = self.pane.index + 1 - h;
        }
        let w = self.pane.w as usize;
        let rows: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let line = crate::truncate_ansi(l, w);
                if i == self.pane.index {
                    select_bar(&line, w)
                } else {
                    line
                }
            })
            .collect();
        self.pane.set_text(&rows.join("\n"));
        self.pane.refresh();
    }

    /// Show the popup (non-blocking, for manual control)
    pub fn show(&mut self, content: &str) {
        self.pane.set_text(content);
        self.pane.border_refresh();
        self.pane.refresh();
    }

    /// Dismiss the popup and refresh underlying panes.
    ///
    /// The border ring sits outside the content area, and `clear` blanks
    /// the content only. A border cell in a column no pane owns, the gap
    /// between two panes for instance, is repainted by nobody and stays
    /// on screen. So the ring is blanked here before the panes redraw.
    pub fn dismiss(&mut self, refresh_panes: &mut [&mut Pane]) {
        self.pane.clear();
        if self.pane.border {
            let (x, y, w, h) = (self.pane.x, self.pane.y, self.pane.w, self.pane.h);
            let (x0, y0) = (x.saturating_sub(1).max(1), y.saturating_sub(1).max(1));
            let row = " ".repeat(w as usize + 2);
            print!("\x1b[0m\x1b[{};{}H{}\x1b[{};{}H{}", y0, x0, row, y + h, x0, row);
            for r in y..y + h {
                print!("\x1b[{};{}H \x1b[{};{}H ", r, x0, r, x + w);
            }
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
        for pane in refresh_panes.iter_mut() {
            pane.full_refresh();
        }
    }
}
