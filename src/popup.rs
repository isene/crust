//! Popup - modal dialog widget, equivalent to rcurses Popup
//!
//! A centered (or positioned) pane that overlays content with keyboard navigation.

use crate::{Pane, Input};
use crate::pane::Align;

pub struct Popup {
    pub pane: Pane,
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

    /// Show the popup with content, return selected line index on Enter, None on ESC
    pub fn modal(&mut self, content: &str) -> Option<usize> {
        self.pane.set_text(content);
        self.pane.ix = 0;
        self.pane.index = 0;
        self.pane.border_refresh();
        self.pane.refresh();

        loop {
            if let Some(key) = Input::getchr(None) {
                match key.as_str() {
                    "ESC" | "q" => return None,
                    "ENTER" => return Some(self.pane.index),
                    "UP" | "k" => {
                        if self.pane.index > 0 {
                            self.pane.index -= 1;
                            // Auto-scroll
                            if self.pane.index < self.pane.ix {
                                self.pane.ix = self.pane.index;
                            }
                            self.pane.refresh();
                        }
                    }
                    "DOWN" | "j" => {
                        let lc = self.pane.line_count();
                        if self.pane.index < lc.saturating_sub(1) {
                            self.pane.index += 1;
                            let visible = (self.pane.h.saturating_sub(2)) as usize;
                            if self.pane.index >= self.pane.ix + visible {
                                self.pane.ix = self.pane.index - visible + 1;
                            }
                            self.pane.refresh();
                        }
                    }
                    "PgDOWN" | " " => self.pane.pagedown(),
                    "PgUP" => self.pane.pageup(),
                    "HOME" | "g" => self.pane.top(),
                    "END" | "G" => self.pane.bottom(),
                    _ => {}
                }
            }
        }
    }

    /// Show the popup (non-blocking, for manual control)
    pub fn show(&mut self, content: &str) {
        self.pane.set_text(content);
        self.pane.border_refresh();
        self.pane.refresh();
    }

    /// Dismiss the popup and refresh underlying panes
    pub fn dismiss(&mut self, refresh_panes: &mut [&mut Pane]) {
        self.pane.clear();
        for pane in refresh_panes.iter_mut() {
            pane.full_refresh();
        }
    }
}
