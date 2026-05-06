//! Pane - the core widget, equivalent to rcurses Pane class
//!
//! A positioned rectangle with content, scrolling, borders, and diff-based rendering.

use crate::{display_width, strip_ansi, truncate_ansi};
use std::io::{self, Write};

/// A terminal pane with position, size, content, and rendering state
pub struct Pane {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub fg: u16,
    pub bg: u16,
    pub border: bool,
    pub border_fg: Option<u16>,  // Custom border color (falls back to fg)
    pub scroll: bool,
    pub scroll_fg: Option<u16>,  // Custom scroll indicator color
    pub align: Align,
    pub ix: usize,
    pub index: usize,
    pub prompt: String,
    pub record: bool,
    pub history: Vec<String>,
    pub moreup: bool,
    pub moredown: bool,
    pub update: bool,  // Flag for conditional rendering
    pub wrap: bool,    // Word-wrap long lines (default true)
    /// When true, `editline` masks each typed character with `•` so
    /// the user's input isn't visible (password prompts). Cursor
    /// movement, backspace, paste, and the returned `String` all
    /// operate on the real chars; only the on-screen rendering is
    /// substituted. Caller is responsible for clearing the flag
    /// after the prompt completes.
    pub secret: bool,
    /// Word-wrap (`true`, the default) breaks long lines at the last
    /// space before the pane edge — ideal for prose preview / status
    /// readability. Char-wrap (`false`) breaks at exactly `width`
    /// chars regardless of token boundaries. Char-wrap is the right
    /// choice for any pane whose caller does its own cursor-position
    /// math against `width`, since word-wrap produces shorter rows
    /// that the caller can't predict without re-running the algorithm.
    pub word_wrap: bool,

    text: String,
    line_count: Option<usize>,
    prev_frame: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align { Left, Center, Right }

impl Pane {
    /// Create a new pane at position (x, y) with size (w, h) and colors
    pub fn new(x: u16, y: u16, w: u16, h: u16, fg: u16, bg: u16) -> Self {
        Self {
            x, y, w, h, fg, bg,
            border: false,
            border_fg: None,
            scroll: true,
            scroll_fg: None,
            align: Align::Left,
            ix: 0,
            index: 0,
            prompt: String::new(),
            record: false,
            history: Vec::new(),
            moreup: false,
            moredown: false,
            update: true,
            wrap: true,
            secret: false,
            word_wrap: true,
            text: String::new(),
            line_count: None,
            prev_frame: Vec::new(),
        }
    }

    /// Set pane text content (invalidates line_count cache).
    ///
    /// Does NOT auto-push the previous text into `history`. The earlier
    /// auto-push behavior corrupted editline history for any app that
    /// reused a single pane as both a status bar and a `:` prompt
    /// (every `say()` shoved the styled status string into the prompt's
    /// history; recalling with Up/Down replayed SGR-fragments instead of
    /// the user's commands). Editline's tail still pushes the user-typed
    /// `buf` explicitly when Enter is hit, so genuine command history
    /// continues to work — only the spurious pollution is gone.
    ///
    /// Apps that want to populate `history` outside an editline session
    /// should push to `pane.history` directly.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.line_count = None;
    }

    /// Get pane text content
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get line count (cached)
    pub fn line_count(&mut self) -> usize {
        if let Some(c) = self.line_count {
            return c;
        }
        let c = self.text.matches('\n').count() + 1;
        self.line_count = Some(c);
        c
    }

    /// Set text and refresh (like rcurses say - resets scroll)
    pub fn say(&mut self, text: &str) {
        self.set_text(text);
        self.ix = 0;
        self.refresh();
    }

    /// Clear content
    pub fn clear(&mut self) {
        self.set_text("");
        self.ix = 0;
        // Clear the pane area on screen
        let (content_x, content_y, content_w, content_h) = self.content_area();
        let blank = " ".repeat(content_w as usize);
        let bg_code = format!("\x1b[48;5;{}m", self.bg);
        for row in 0..content_h {
            print!("\x1b[{};{}H{}{}\x1b[0m", content_y + row, content_x, bg_code, blank);
        }
        io::stdout().flush().ok();
        self.prev_frame.clear();
    }

    /// Diff-based refresh: only redraws changed lines
    pub fn refresh(&mut self) {
        let (cx, cy, cw, ch) = self.content_area();
        let lines = self.wrap_lines(cw as usize);
        let total = lines.len();

        // Update scroll state
        self.moreup = self.ix > 0;
        self.moredown = self.ix + (ch as usize) < total;

        // Build the frame
        let mut frame: Vec<String> = Vec::with_capacity(ch as usize);
        for i in 0..ch as usize {
            let line_idx = self.ix + i;
            if line_idx < lines.len() {
                let line = &lines[line_idx];
                let aligned = self.align_line(line, cw as usize);
                frame.push(aligned);
            } else {
                frame.push(String::new());
            }
        }

        // Diff render: only write changed lines
        let bg_code = format!("\x1b[48;5;{}m", self.bg);
        let fg_code = format!("\x1b[38;5;{}m", self.fg);
        let pane_colors = format!("{}{}", bg_code, fg_code);

        for (i, line) in frame.iter().enumerate() {
            let changed = i >= self.prev_frame.len() || self.prev_frame[i] != *line;
            if changed {
                let row = cy + i as u16;
                let expanded = line.replace('\t', "        ");
                let has_ansi = expanded.contains("\x1b[");
                let has_bg = has_ansi && has_ansi_bg(&expanded);
                let max_w = cw as usize;

                // Match rcurses: 3 branches based on ANSI content
                let processed = if has_ansi && has_bg {
                    // Line has its own bg colors: preserve them, apply pane bg only to padding
                    let restored = expanded.replace("\x1b[0m", &format!("\x1b[0m{}", pane_colors));
                    restored
                } else if has_ansi {
                    // Line has ANSI codes but no bg: strip bg, apply pane colors
                    let no_bg = strip_ansi_bg(&expanded);
                    let restored = no_bg.replace("\x1b[0m", &format!("\x1b[0m{}", pane_colors));
                    restored
                } else {
                    // No ANSI: plain text, pane_colors prefix handles coloring
                    expanded
                };

                let vis_len = display_width(&processed);
                let clipped = if vis_len > max_w {
                    truncate_ansi(&processed, max_w)
                } else {
                    processed
                };
                let clipped_len = display_width(&clipped);
                let pad = if clipped_len < max_w {
                    format!("\x1b[0m{}{}", pane_colors, " ".repeat(max_w - clipped_len))
                } else {
                    String::new()
                };
                print!("\x1b[{};{}H{}{}{}\x1b[0m", row, cx, pane_colors, clipped, pad);
            }
        }

        // Scroll indicators — paint on the pane's own bg so the
        // marker doesn't show through to whatever the cell sat on
        // before the pane drew there (e.g. an underlying page when
        // the pane is a centered popup).
        if self.scroll {
            let sc = self.scroll_fg.unwrap_or(self.fg);
            if self.moreup {
                print!("\x1b[{};{}H\x1b[38;5;{}m\x1b[48;5;{}m\u{25B3}\x1b[0m",
                    cy, cx + cw - 1, sc, self.bg);
            }
            if self.moredown {
                print!("\x1b[{};{}H\x1b[38;5;{}m\x1b[48;5;{}m\u{25BD}\x1b[0m",
                    cy + ch - 1, cx + cw - 1, sc, self.bg);
            }
        }

        io::stdout().flush().ok();
        self.prev_frame = frame;
    }

    /// Mark the diff cache stale without writing anything. Use after the
    /// host code wipes the screen externally (Crust::clear_screen, alt-screen
    /// switch on subprocess return) so the next `say()` / `refresh()` sees
    /// every line as changed and repaints. Cheaper than `full_refresh` when
    /// the caller is about to call `say()` anyway with fresh content.
    pub fn invalidate(&mut self) {
        self.prev_frame.clear();
    }

    /// Force complete repaint (clears diff cache, redraws border)
    pub fn full_refresh(&mut self) {
        self.prev_frame.clear();
        // Clear the entire pane area first to prevent color artifacts from old content
        let (cx, cy, cw, ch) = self.content_area();
        let bg_code = format!("\x1b[48;5;{}m", self.bg);
        let blank = " ".repeat(cw as usize);
        for row in 0..ch {
            print!("\x1b[{};{}H{}{}\x1b[0m", cy + row, cx, bg_code, blank);
        }
        if self.border {
            self.draw_border();
        }
        self.refresh();
    }

    /// Efficient scroll by N lines using terminal scroll regions.
    /// Only renders the newly exposed line(s) instead of the full pane.
    /// `delta`: positive = scroll down (content moves up), negative = scroll up.
    /// Falls back to refresh() if delta is too large or prev_frame is empty.
    pub fn scroll_refresh(&mut self, delta: i32) {
        let (cx, cy, cw, ch) = self.content_area();
        let h = ch as usize;
        let abs = delta.unsigned_abs() as usize;

        // Fall back to full diff render if delta is large or no previous frame
        if abs == 0 || abs >= h || self.prev_frame.len() != h {
            self.refresh();
            return;
        }

        let lines = self.wrap_lines(cw as usize);
        let total = lines.len();
        self.moreup = self.ix > 0;
        self.moredown = self.ix + h < total;

        let bg_code = format!("\x1b[48;5;{}m", self.bg);
        let fg_code = format!("\x1b[38;5;{}m", self.fg);
        let pane_colors = format!("{}{}", bg_code, fg_code);

        // Clear old scroll indicators before shifting (they'd ghost otherwise)
        let indicator_col = cx + cw - 1;
        if self.scroll {
            print!("\x1b[{};{}H \x1b[{};{}H ", cy, indicator_col, cy + ch - 1, indicator_col);
        }

        // Set scroll region to pane area
        let top_row = cy;
        let bot_row = cy + ch - 1;
        print!("\x1b[{};{}r", top_row, bot_row);

        if delta > 0 {
            // Scroll down: content moves up, new lines appear at bottom
            print!("\x1b[{};1H", bot_row);
            for _ in 0..abs { print!("\n"); }
        } else {
            // Scroll up: content moves down, new lines appear at top
            print!("\x1b[{};1H", top_row);
            for _ in 0..abs { print!("\x1bM"); } // reverse index
        }

        // Reset scroll region
        print!("\x1b[r");

        // Render only the newly exposed lines
        if delta > 0 {
            // New lines at bottom
            let start = h - abs;
            for i in start..h {
                let line_idx = self.ix + i;
                let content = if line_idx < lines.len() {
                    self.align_line(&lines[line_idx], cw as usize)
                } else {
                    String::new()
                };
                self.render_pane_line(cy + i as u16, cx, cw, &pane_colors, &content);
            }
        } else {
            // New lines at top
            for i in 0..abs {
                let line_idx = self.ix + i;
                let content = if line_idx < lines.len() {
                    self.align_line(&lines[line_idx], cw as usize)
                } else {
                    String::new()
                };
                self.render_pane_line(cy + i as u16, cx, cw, &pane_colors, &content);
            }
        }

        // Scroll indicators (full_refresh path) — same pane-bg fix
        // as the diff-render branch above.
        if self.scroll {
            let sc = self.scroll_fg.unwrap_or(self.fg);
            if self.moreup {
                print!("\x1b[{};{}H\x1b[38;5;{}m\x1b[48;5;{}m\u{2206}\x1b[0m",
                    cy, cx + cw - 1, sc, self.bg);
            }
            if self.moredown {
                print!("\x1b[{};{}H\x1b[38;5;{}m\x1b[48;5;{}m\u{2207}\x1b[0m",
                    cy + ch - 1, cx + cw - 1, sc, self.bg);
            }
        }

        io::stdout().flush().ok();

        // Update prev_frame
        let mut frame: Vec<String> = Vec::with_capacity(h);
        for i in 0..h {
            let line_idx = self.ix + i;
            if line_idx < lines.len() {
                frame.push(self.align_line(&lines[line_idx], cw as usize));
            } else {
                frame.push(String::new());
            }
        }
        self.prev_frame = frame;
    }

    /// Render a single pane line at the given row
    fn render_pane_line(&self, row: u16, cx: u16, cw: u16, pane_colors: &str, content: &str) {
        let expanded = content.replace('\t', "        ");
        let has_ansi = expanded.contains("\x1b[");
        let has_bg = has_ansi && has_ansi_bg(&expanded);
        let max_w = cw as usize;

        let processed = if has_ansi && has_bg {
            // Content has its own bg: preserve it, apply pane bg only to padding
            expanded.replace("\x1b[0m", &format!("\x1b[0m{}", pane_colors))
        } else if has_ansi {
            // Content has ANSI but no bg: strip bg artifacts, apply pane colors
            let no_bg = strip_ansi_bg(&expanded);
            no_bg.replace("\x1b[0m", &format!("\x1b[0m{}", pane_colors))
        } else {
            expanded
        };

        let vis_len = display_width(&processed);
        let clipped = if vis_len > max_w {
            truncate_ansi(&processed, max_w)
        } else {
            processed
        };
        let clipped_len = display_width(&clipped);
        let pad = if clipped_len < max_w {
            format!("\x1b[0m{}{}", pane_colors, " ".repeat(max_w - clipped_len))
        } else { String::new() };
        print!("\x1b[{};{}H{}{}{}\x1b[0m", row, cx, pane_colors, clipped, pad);
    }

    /// Refresh border only
    pub fn border_refresh(&mut self) {
        if self.border {
            self.draw_border();
        }
    }

    /// Erase the border glyphs around this pane (corners + top/bottom rows
    /// + side columns). Used when focus moves away and the caller no longer
    /// wants the pane to be framed. Paints spaces with the default terminal
    /// background so it blends into whatever sits outside the pane.
    pub fn border_clear(&mut self) {
        let (x, y, w, h) = (self.x, self.y, self.w, self.h);
        let left = x.saturating_sub(1);
        let top = y.saturating_sub(1);
        let right = x + w;
        let bottom = y + h;
        let hbar = " ".repeat(w as usize);
        print!("\x1b[{};{}H\x1b[0m {}{} ", top, left, hbar, " ");
        print!("\x1b[{};{}H\x1b[0m {}{} ", bottom, left, hbar, " ");
        for row in 0..h {
            print!("\x1b[{};{}H\x1b[0m ", y + row, left);
            print!("\x1b[{};{}H\x1b[0m ", y + row, right);
        }
        io::stdout().flush().ok();
    }

    /// Scroll up one line
    pub fn lineup(&mut self) {
        if self.ix > 0 {
            self.ix -= 1;
            self.refresh();
        }
    }

    /// Scroll down one line
    pub fn visual_line_count(&self) -> usize {
        let (_, _, cw, _) = self.content_area();
        self.wrap_lines(cw as usize).len()
    }

    pub fn linedown(&mut self) {
        let total = self.visual_line_count();
        if self.ix < total.saturating_sub(1) {
            self.ix += 1;
            self.refresh();
        }
    }

    /// Scroll up one page
    pub fn pageup(&mut self) {
        let (_, _, _, ch) = self.content_area();
        let page = ch as usize;
        self.ix = self.ix.saturating_sub(page.saturating_sub(1));
        self.refresh();
    }

    /// Scroll down one page. `ix` is a visual-row index (see refresh), so clamp
    /// against the wrapped line count, not the logical line count — otherwise
    /// wrapped content stops pagedown short of the true bottom.
    pub fn pagedown(&mut self) {
        let (_, _, cw, ch) = self.content_area();
        let total = self.wrap_lines(cw as usize).len();
        let page = ch as usize;
        self.ix = (self.ix + page.saturating_sub(1)).min(total.saturating_sub(page));
        self.refresh();
    }

    /// Scroll to top
    pub fn top(&mut self) {
        self.ix = 0;
        self.refresh();
    }

    /// Scroll to bottom
    pub fn bottom(&mut self) {
        let (_, _, cw, ch) = self.content_area();
        let total = self.wrap_lines(cw as usize).len();
        let page = ch as usize;
        self.ix = total.saturating_sub(page);
        self.refresh();
    }

    /// Move pane by relative amounts
    pub fn move_by(&mut self, dx: i16, dy: i16) {
        self.x = (self.x as i16 + dx).max(1) as u16;
        self.y = (self.y as i16 + dy).max(1) as u16;
        self.full_refresh();
    }

    /// Ask for input (prompt + initial text, returns edited text)
    pub fn ask(&mut self, prompt: &str, initial: &str) -> String {
        self.prompt = prompt.to_string();
        self.set_text(initial);
        self.editline()
    }

    /// Ask with temporary background color (e.g. dark blue for command input)
    pub fn ask_with_bg(&mut self, prompt: &str, initial: &str, temp_bg: u16) -> String {
        let orig_bg = self.bg;
        self.bg = temp_bg;
        let result = self.ask(prompt, initial);
        self.bg = orig_bg;
        result
    }


    /// Single-line editor with history support
    pub fn editline(&mut self) -> String {
        use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
        use crossterm::terminal;

        let (cx, cy, cw, _) = self.content_area();
        let prompt_w = display_width(&self.prompt);
        let edit_w = (cw as usize).saturating_sub(prompt_w);

        let mut buf = self.text.clone();
        let mut cursor = buf.len();
        let mut hist_pos: Option<usize> = None;
        let mut saved = String::new();

        // Draw prompt + initial text
        let redraw = |buf: &str, cursor: usize, prompt: &str, cx: u16, cy: u16, cw: u16, fg: u16, bg: u16, secret: bool| {
            let prompt_w = display_width(prompt);
            let edit_w = (cw as usize).saturating_sub(prompt_w);
            // Source for visible rendering: real chars unless secret
            // mode, in which case substitute each char with U+2022 so
            // the password isn't shown. Slicing happens in char space
            // either way to preserve UTF-8 boundaries.
            let source: String = if secret {
                buf.chars().map(|_| '\u{2022}').collect()
            } else {
                buf.to_string()
            };
            let visible: String = if source.chars().count() > edit_w {
                source.chars().skip(source.chars().count() - edit_w).collect()
            } else {
                source
            };
            let pad = " ".repeat(edit_w.saturating_sub(display_width(&visible)));
            print!("\x1b[{};{}H\x1b[48;5;{}m\x1b[38;5;{}m{}{}{}\x1b[0m",
                cy, cx, bg, fg, prompt, visible, pad);
            // Position cursor by display width of the chars before it.
            // In secret mode every char is one column wide (•), so
            // count chars instead of measuring the real (hidden) text.
            let safe_cursor = cursor.min(buf.len());
            let cursor_w = if secret {
                buf[..safe_cursor].chars().count()
            } else {
                let prefix = if buf.is_char_boundary(safe_cursor) {
                    &buf[..safe_cursor]
                } else { "" };
                display_width(prefix)
            };
            let cursor_col = cx + prompt_w as u16 + cursor_w as u16;
            print!("\x1b[{};{}H", cy, cursor_col);
            io::stdout().flush().ok();
        };

        // Step the byte cursor backwards/forwards by one full char.
        let prev_boundary = |s: &str, c: usize| -> usize {
            if c == 0 { return 0; }
            let mut i = c - 1;
            while i > 0 && !s.is_char_boundary(i) { i -= 1; }
            i
        };
        let next_boundary = |s: &str, c: usize| -> usize {
            if c >= s.len() { return s.len(); }
            let mut i = c + 1;
            while i < s.len() && !s.is_char_boundary(i) { i += 1; }
            i
        };

        crossterm::execute!(io::stdout(), crossterm::cursor::Show).ok();
        redraw(&buf, cursor, &self.prompt, cx, cy, cw, self.fg, self.bg, self.secret);

        loop {
            let ev = match event::read() {
                Ok(ev) => ev,
                Err(_) => break,
            };
            match ev {
                Event::Key(KeyEvent { code, modifiers, .. }) => {
                    match (code, modifiers) {
                        (KeyCode::Enter, _) => break,
                        (KeyCode::Esc, _) => {
                            buf.clear();
                            break;
                        }
                        (KeyCode::Backspace, _) => {
                            if cursor > 0 {
                                let new_cursor = prev_boundary(&buf, cursor);
                                buf.replace_range(new_cursor..cursor, "");
                                cursor = new_cursor;
                            }
                        }
                        (KeyCode::Delete, _) => {
                            if cursor < buf.len() {
                                let next = next_boundary(&buf, cursor);
                                buf.replace_range(cursor..next, "");
                            }
                        }
                        (KeyCode::Left, _) => {
                            cursor = prev_boundary(&buf, cursor);
                        }
                        (KeyCode::Right, _) => {
                            cursor = next_boundary(&buf, cursor);
                        }
                        (KeyCode::Home, _) => cursor = 0,
                        (KeyCode::End, _) => cursor = buf.len(),
                        (KeyCode::Up, _) if self.record && !self.history.is_empty() => {
                            let pos = match hist_pos {
                                Some(p) => (p + 1).min(self.history.len() - 1),
                                None => { saved = buf.clone(); 0 }
                            };
                            hist_pos = Some(pos);
                            buf = self.history[self.history.len() - 1 - pos].clone();
                            cursor = buf.len();
                        }
                        (KeyCode::Down, _) if self.record => {
                            match hist_pos {
                                Some(0) => {
                                    hist_pos = None;
                                    buf = saved.clone();
                                    cursor = buf.len();
                                }
                                Some(p) => {
                                    hist_pos = Some(p - 1);
                                    buf = self.history[self.history.len() - p].clone();
                                    cursor = buf.len();
                                }
                                None => {}
                            }
                        }
                        (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                            buf.truncate(cursor);
                        }
                        (KeyCode::Char(c), _) if c != '\t' => {
                            buf.insert(cursor, c);
                            cursor += c.len_utf8();
                        }
                        _ => {}
                    }
                }
                // Bracketed paste — apps that enabled `\x1b[?2004h` get
                // multi-byte pastes as a single Paste event. Insert the
                // payload at the cursor, but flatten any embedded newlines
                // to spaces so a stray multi-line paste doesn't commit
                // the prompt mid-paste.
                Event::Paste(s) => {
                    let cleaned: String = s.replace(['\r', '\n'], " ");
                    buf.insert_str(cursor, &cleaned);
                    cursor += cleaned.len();
                }
                _ => {}
            }
            redraw(&buf, cursor, &self.prompt, cx, cy, cw, self.fg, self.bg, self.secret);
        }

        crossterm::execute!(io::stdout(), crossterm::cursor::Hide).ok();

        if self.record && !buf.is_empty() && !self.secret {
            self.history.push(buf.clone());
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
        // Never retain a secret input as pane state — the caller already
        // owns the returned String.
        self.text = if self.secret { String::new() } else { buf.clone() };
        // Editline did its own raw prints throughout the loop; the pane's
        // prev_frame still reflects whatever was on screen BEFORE ask was
        // called. Invalidate it so the next say()/refresh() does a full
        // repaint — otherwise the diff-render says "no change" and leaves
        // the now-stale prompt + temp bg on screen (visible after ESC,
        // when the caller restores the regular status bar).
        self.prev_frame.clear();
        buf
    }

    /// Clean up caches and history
    pub fn cleanup(&mut self) {
        self.prev_frame.clear();
        self.history.clear();
        self.line_count = None;
    }

    // --- Private helpers ---

    /// Content area is always (x, y, w, h).
    /// Border is drawn OUTSIDE the content area (like rcurses).
    fn content_area(&self) -> (u16, u16, u16, u16) {
        (self.x, self.y, self.w, self.h)
    }

    /// Word-wrap text to fit width, preserving ANSI codes and OSC 8
    /// hyperlink state. When a wrap splits a line while an OSC 8 hyperlink
    /// is open, the current segment is closed with an empty OSC 8 and the
    /// next segment reopens the same URL. Without this, the hyperlink state
    /// leaks past the pane's trailing padding and kitty-style terminals
    /// render everything downstream (including other panes) with the
    /// url_style underline.
    fn wrap_lines(&self, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![];
        }
        let mut result = Vec::new();
        let expanded_text = self.text.replace('\t', "        ");
        for line in expanded_text.split('\n') {
            if !self.wrap || display_width(line) <= width {
                result.push(line.to_string());
            } else {
                // Word-wrap with ANSI preservation
                let mut current = String::new();
                let mut current_width = 0;
                let mut active_ansi = String::new();
                // OSC 8 link target that's currently open, if any. We need
                // this to re-open the link on each continuation segment.
                let mut active_osc_url: Option<String> = None;

                let chars: Vec<char> = line.chars().collect();
                let mut i = 0;
                while i < chars.len() {
                    // Check for ANSI escape sequence
                    if chars[i] == '\x1b' && i + 1 < chars.len() && chars[i + 1] == '[' {
                        let start = i;
                        i += 2;
                        while i < chars.len() && !chars[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        if i < chars.len() {
                            i += 1; // include the letter
                        }
                        let seq: String = chars[start..i].iter().collect();
                        if seq == "\x1b[0m" {
                            active_ansi.clear();
                        } else {
                            active_ansi = seq.clone();
                        }
                        current.push_str(&seq);
                        continue;
                    }

                    // Check for OSC sequence (e.g. OSC 8 hyperlinks: \x1b]8;;URL\x1b\\)
                    // Terminated by ST (\x1b\\) or BEL (\x07). Zero-width passthrough.
                    if chars[i] == '\x1b' && i + 1 < chars.len() && chars[i + 1] == ']' {
                        let start = i;
                        i += 2;
                        while i < chars.len() {
                            if chars[i] == '\x07' { i += 1; break; }
                            if chars[i] == '\x1b' && i + 1 < chars.len() && chars[i + 1] == '\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                        let seq: String = chars[start..i].iter().collect();
                        // Track OSC 8 open/close so we can re-open on wrap.
                        // Minimal parse: look for `]8;<params>;<URL>` inside seq.
                        if let Some(rest) = seq.strip_prefix("\x1b]8;") {
                            let body = rest.trim_end_matches('\x07');
                            let body = body.strip_suffix("\x1b\\").unwrap_or(body);
                            // body is `<params>;<URL>` (URL empty means close)
                            if let Some(sep) = body.find(';') {
                                let url = &body[sep + 1..];
                                active_osc_url = if url.is_empty() {
                                    None
                                } else {
                                    Some(url.to_string())
                                };
                            }
                        }
                        current.push_str(&seq);
                        continue;
                    }

                    let ch_width = unicode_width::UnicodeWidthChar::width(chars[i]).unwrap_or(1);
                    if current_width + ch_width > width {
                        // If a hyperlink is open, close it before the wrap
                        // boundary so the pane's trailing padding + next
                        // row's leading cells don't inherit the link.
                        let close_osc = if active_osc_url.is_some() {
                            "\x1b]8;;\x1b\\"
                        } else {
                            ""
                        };
                        let reopen_osc = if let Some(ref url) = active_osc_url {
                            format!("\x1b]8;;{}\x1b\\", url)
                        } else {
                            String::new()
                        };
                        // Try to break at last space (word-wrap only)
                        if let Some(space_pos) = if self.word_wrap { current.rfind(' ') } else { None } {
                            let remainder: String = current[space_pos + 1..].to_string();
                            current.truncate(space_pos);
                            current.push_str(close_osc);
                            result.push(current);
                            current = format!("{}{}{}", reopen_osc, active_ansi, remainder);
                            current_width = display_width(&strip_ansi(&current));
                        } else {
                            current.push_str(close_osc);
                            result.push(current);
                            current = format!("{}{}", reopen_osc, active_ansi);
                            current_width = 0;
                        }
                    }

                    current.push(chars[i]);
                    current_width += ch_width;
                    i += 1;
                }
                // Final safety: if an OSC 8 link is still open at line end
                // (caller forgot to close), emit an empty-URL OSC 8 to close.
                if active_osc_url.is_some() {
                    current.push_str("\x1b]8;;\x1b\\");
                }
                if !current.is_empty() || result.is_empty() {
                    result.push(current);
                }
            }
        }
        result
    }

    /// Align a line within the content width
    fn align_line(&self, line: &str, width: usize) -> String {
        let vis_len = display_width(line);
        match self.align {
            Align::Left => line.to_string(),
            Align::Center => {
                if vis_len >= width {
                    line.to_string()
                } else {
                    let pad = (width - vis_len) / 2;
                    format!("{}{}", " ".repeat(pad), line)
                }
            }
            Align::Right => {
                if vis_len >= width {
                    line.to_string()
                } else {
                    format!("{}{}", " ".repeat(width - vis_len), line)
                }
            }
        }
    }

    /// Draw border around pane
    /// Draw border OUTSIDE the pane area (matching rcurses).
    /// Border occupies (x-1, y-1) to (x+w, y+h).
    fn draw_border(&self) {
        let (x, y, w, h) = (self.x, self.y, self.w, self.h);
        let left = x.saturating_sub(1);
        let top = y.saturating_sub(1);
        let right = x + w;
        let bottom = y + h;
        let bfg = self.border_fg.unwrap_or(self.fg);
        let fg_code = format!("\x1b[38;5;{}m", bfg);
        let bg_code = format!("\x1b[48;5;{}m", self.bg);

        // Top border: from (left, top) to (right, top)
        let hbar = "\u{2500}".repeat(w as usize);
        print!("\x1b[{};{}H{}{}\u{250c}{}\u{2510}",
            top, left, fg_code, bg_code, hbar);
        // Bottom border
        print!("\x1b[{};{}H{}{}\u{2514}{}\u{2518}",
            bottom, left, fg_code, bg_code, hbar);
        // Side borders
        for row in 0..h {
            print!("\x1b[{};{}H{}{}\u{2502}", y + row, left, fg_code, bg_code);
            print!("\x1b[{};{}H\u{2502}", y + row, right);
        }
        print!("\x1b[0m");
        io::stdout().flush().ok();
    }
}

/// Check if a string contains ANSI background color sequences (48;5;N or 48;2;R;G;B).
/// Matches rcurses' /\e\[[\d;]*48;[25];/ pattern.
fn has_ansi_bg(s: &str) -> bool {
    // Look for \e[...48;5; or \e[...48;2; patterns
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len.saturating_sub(5) {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            // Found ESC[, scan for 48;5; or 48;2; before the terminator
            let mut j = i + 2;
            while j < len && (bytes[j].is_ascii_digit() || bytes[j] == b';') {
                j += 1;
            }
            let params = &s[i + 2..j];
            if params.contains("48;5;") || params.contains("48;2;") {
                return true;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    false
}

/// Strip ANSI background color sequences from a string.
/// Preserves foreground colors, text attributes, and all UTF-8 content.
fn strip_ansi_bg(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        if ch == '\x1b' {
            // Check for CSI sequence: \x1b[...m
            if let Some(&(_, '[')) = chars.peek() {
                chars.next(); // consume '['
                // Collect parameter string until we hit a letter
                let param_start = if let Some(&(p, _)) = chars.peek() { p } else { continue };
                let mut end_pos = param_start;
                let mut terminator = ' ';
                while let Some(&(p, c)) = chars.peek() {
                    end_pos = p;
                    if c.is_ascii_alphabetic() {
                        terminator = c;
                        chars.next(); // consume terminator
                        break;
                    }
                    chars.next();
                }
                if terminator == 'm' {
                    let params = &s[param_start..end_pos];
                    // Check if sequence contains bg codes
                    let has_bg = params.contains("48;") || params.split(';').any(|p| {
                        matches!(p.parse::<u32>(), Ok(40..=47) | Ok(49) | Ok(100..=107))
                    });
                    if has_bg {
                        // Rebuild without bg codes
                        let parts: Vec<&str> = params.split(';').collect();
                        let mut keep = Vec::new();
                        let mut i = 0;
                        while i < parts.len() {
                            let code: u32 = parts[i].parse().unwrap_or(999);
                            match code {
                                40..=47 | 49 | 100..=107 => { i += 1; }
                                48 => {
                                    // Skip 48;5;N or 48;2;R;G;B
                                    if i + 2 < parts.len() && parts[i + 1] == "5" { i += 3; }
                                    else if i + 4 < parts.len() && parts[i + 1] == "2" { i += 5; }
                                    else { i += 1; }
                                }
                                38 => {
                                    // Keep fg: 38;5;N or 38;2;R;G;B
                                    if i + 2 < parts.len() && parts[i + 1] == "5" {
                                        keep.extend_from_slice(&parts[i..i+3]); i += 3;
                                    } else if i + 4 < parts.len() && parts[i + 1] == "2" {
                                        keep.extend_from_slice(&parts[i..i+5]); i += 5;
                                    } else { keep.push(parts[i]); i += 1; }
                                }
                                _ => { keep.push(parts[i]); i += 1; }
                            }
                        }
                        if !keep.is_empty() {
                            result.push_str("\x1b[");
                            result.push_str(&keep.join(";"));
                            result.push('m');
                        }
                    } else {
                        // No bg codes: keep entire sequence
                        result.push_str(&s[pos..end_pos]);
                        result.push(terminator);
                    }
                } else {
                    // Non-m terminator: keep as-is
                    result.push_str(&s[pos..end_pos]);
                    result.push(terminator);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    result
}
