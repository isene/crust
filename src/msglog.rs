//! A short history of the status lines an app has already shown.
//!
//! A status line says one thing and is gone: a send that failed, a
//! source that stopped working, a reply from a background job. Look
//! away for a moment and it is unrecoverable. This keeps the last few
//! so the user can read what they missed.
//!
//! Ages are relative, so no clock or timezone is involved:
//!
//! ```no_run
//! let mut pane = crust::Pane::new(1, 1, 80, 24, 255, 0);
//! let mut log = crust::MessageLog::new(10);
//! log.push("Sent to alice", 46);
//! log.show("Messages", 255, 235, &mut [&mut pane]);
//! ```

use std::collections::VecDeque;
use std::time::Instant;

use crate::{style, Pane, Popup};

pub struct MessageLog {
    cap: usize,
    entries: VecDeque<(Instant, String, u8)>,
}

/// "just now", "12s", "4m", "2h", "3d".
fn age(since: Instant) -> String {
    let s = since.elapsed().as_secs();
    if s < 5 { "just now".to_string() }
    else if s < 60 { format!("{}s ago", s) }
    else if s < 3600 { format!("{}m ago", s / 60) }
    else if s < 86400 { format!("{}h ago", s / 3600) }
    else { format!("{}d ago", s / 86400) }
}

impl MessageLog {
    /// Keep the last `cap` messages. A cap of 0 keeps none, and every
    /// other call becomes a no-op.
    pub fn new(cap: usize) -> Self {
        Self { cap, entries: VecDeque::new() }
    }

    /// Change how many are kept, dropping the oldest if that is fewer.
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap;
        while self.entries.len() > self.cap { self.entries.pop_front(); }
    }

    /// Remember one message and the color it was shown in.
    pub fn push(&mut self, msg: &str, fg: u8) {
        if self.cap == 0 { return; }
        if self.entries.len() == self.cap { self.entries.pop_front(); }
        self.entries.push_back((Instant::now(), msg.to_string(), fg));
    }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn clear(&mut self) { self.entries.clear(); }

    /// The log as lines, newest last, each in the color it was shown in.
    pub fn render(&self) -> String {
        if self.entries.is_empty() {
            return style::fg("  (nothing yet)", 244);
        }
        let width = self.entries.iter()
            .map(|(t, _, _)| crate::display_width(&age(*t)))
            .max().unwrap_or(0);
        self.entries.iter().map(|(t, msg, fg)| {
            let stamp = age(*t);
            let pad = " ".repeat(width - crate::display_width(&stamp));
            format!("  {}{}  {}", pad, style::fg(&stamp, 244), style::fg(msg, *fg))
        }).collect::<Vec<_>>().join("\n")
    }

    /// Open the log in a popup, sized to what it holds. Any of ESC, q
    /// or ENTER closes it.
    ///
    /// `restore` is every pane the popup covered. A pane redraws only
    /// what changed since its last frame, and it never learns that a
    /// popup wrote over it, so without this the popup's border and
    /// text stay on screen after it closes.
    pub fn show(&self, title: &str, fg: u16, bg: u16, restore: &mut [&mut Pane]) {
        let body = self.render();
        let content = format!("{}\n\n{}", style::bold(title), body);
        let (cols, rows) = crate::Crust::terminal_size();
        let widest = content.split('\n')
            .map(|l| crate::display_width(l)).max().unwrap_or(20);
        let w = (widest as u16 + 6).min(cols.saturating_sub(4)).max(24);
        let h = (content.split('\n').count() as u16 + 4).min(rows.saturating_sub(4));
        let mut popup = Popup::centered(w, h, fg, bg);
        popup.view(&content);
        popup.dismiss(restore);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_drops_the_oldest() {
        let mut log = MessageLog::new(3);
        for m in ["one", "two", "three", "four"] { log.push(m, 46); }
        let out = log.render();
        assert!(!out.contains("one"), "oldest survived: {}", out);
        assert!(out.contains("four"), "newest missing: {}", out);
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn a_cap_of_zero_keeps_nothing() {
        let mut log = MessageLog::new(0);
        log.push("ignored", 46);
        assert!(log.is_empty());
        assert!(log.render().contains("nothing yet"));
    }

    #[test]
    fn shrinking_the_cap_drops_the_oldest() {
        let mut log = MessageLog::new(5);
        for m in ["a", "b", "c", "d", "e"] { log.push(m, 46); }
        log.set_cap(2);
        let out = log.render();
        assert_eq!(log.len(), 2);
        assert!(out.contains("d") && out.contains("e"), "{}", out);
    }
}
