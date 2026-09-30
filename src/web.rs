//! crust in a web page: the few pieces of crossterm crust uses, for a build
//! for WASI (`--target wasm32-wasip1`). The page's script runs the program
//! in a terminal drawn by xterm.js (see web/ in this repo).
//!
//! Output is plain stdout, as in a real terminal. Keys come from the page
//! through two imports in module `crust`:
//!
//! - `key(buf, len, ms)` waits up to `ms` milliseconds (-1: for ever) for
//!   the next event and writes it into `buf`: `k<mods> <key>` for a key,
//!   with the browser's own key name ("ArrowUp", "a", "F5") and mods as
//!   1 Shift, 2 Alt, 4 Ctrl; `r` when the terminal changed size; `p<text>`
//!   for a paste. It returns the length, 0 when the wait ran out.
//! - `size()` gives the columns in the high 16 bits, the rows in the low.
//!
//! A wait for a key has to give the page back its thread, so the build is
//! run through binaryen's asyncify, which lets `key` pause the program.

use std::cell::RefCell;
use std::io::{self, Write};
use std::time::Duration;

#[cfg(target_os = "wasi")]
#[link(wasm_import_module = "crust")]
extern "C" {
    #[link_name = "key"]
    fn page_key(buf: *mut u8, len: u32, ms: i32) -> i32;
    #[link_name = "size"]
    fn page_size() -> u32;
}

// For the tests, run natively: no page, no keys, 80 by 24.
#[cfg(not(target_os = "wasi"))]
unsafe fn page_key(_buf: *mut u8, _len: u32, _ms: i32) -> i32 { 0 }
#[cfg(not(target_os = "wasi"))]
unsafe fn page_size() -> u32 { (80 << 16) | 24 }

/// What a crossterm command writes, here for the page's terminal.
pub trait Command {
    fn ansi(&self) -> &'static str;
}

macro_rules! execute {
    ($out:expr $(, $cmd:expr)* $(,)?) => {{
        use std::io::Write;
        let out = &mut $out;
        $( let _ = out.write_all($crate::crossterm::Command::ansi(&$cmd).as_bytes()); )*
        out.flush()
    }};
}
pub(crate) use execute;

pub mod terminal {
    use super::Command;

    pub fn enable_raw_mode() -> std::io::Result<()> { Ok(()) }
    pub fn disable_raw_mode() -> std::io::Result<()> { Ok(()) }

    pub fn size() -> std::io::Result<(u16, u16)> {
        let s = unsafe { super::page_size() };
        Ok(((s >> 16) as u16, s as u16))
    }

    pub struct EnterAlternateScreen;
    pub struct LeaveAlternateScreen;
    impl Command for EnterAlternateScreen { fn ansi(&self) -> &'static str { "\x1b[?1049h" } }
    impl Command for LeaveAlternateScreen { fn ansi(&self) -> &'static str { "\x1b[?1049l" } }
}

pub mod cursor {
    use super::Command;

    pub struct Hide;
    pub struct Show;
    impl Command for Hide { fn ansi(&self) -> &'static str { "\x1b[?25l" } }
    impl Command for Show { fn ansi(&self) -> &'static str { "\x1b[?25h" } }

    /// The page's terminal is not asked where its cursor is.
    pub fn position() -> std::io::Result<(u16, u16)> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no cursor query in a web page"))
    }
}

// Some of this is here only to match crossterm, and goes unused.
#[allow(dead_code)]
pub mod event {
    use super::Command;
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum KeyCode {
        Backspace, Enter, Left, Right, Up, Down, Home, End, PageUp, PageDown,
        Tab, BackTab, Delete, Insert, F(u8), Char(char), Esc, Null,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct KeyModifiers(pub(crate) u8);
    impl KeyModifiers {
        pub const NONE: KeyModifiers = KeyModifiers(0);
        pub const SHIFT: KeyModifiers = KeyModifiers(1);
        pub const ALT: KeyModifiers = KeyModifiers(2);
        pub const CONTROL: KeyModifiers = KeyModifiers(4);
        pub fn contains(self, other: KeyModifiers) -> bool { self.0 & other.0 == other.0 }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum KeyEventKind { Press, Repeat, Release }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct KeyEvent {
        pub code: KeyCode,
        pub modifiers: KeyModifiers,
        pub kind: KeyEventKind,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Event {
        Key(KeyEvent),
        Resize(u16, u16),
        Paste(String),
        FocusGained,
        FocusLost,
    }

    /// Has an event come within `timeout`? It waits in the page.
    pub fn poll(timeout: Duration) -> std::io::Result<bool> {
        super::poll(timeout)
    }

    /// The next event, waiting for it as long as it takes.
    pub fn read() -> std::io::Result<Event> {
        super::read()
    }

    // The keyboard modes a terminal can be asked for: the page reports
    // keys its own way, so these write nothing.
    #[derive(Clone, Copy)]
    pub struct KeyboardEnhancementFlags(u8);
    impl KeyboardEnhancementFlags {
        pub const DISAMBIGUATE_ESCAPE_CODES: KeyboardEnhancementFlags = KeyboardEnhancementFlags(1);
        pub const REPORT_EVENT_TYPES: KeyboardEnhancementFlags = KeyboardEnhancementFlags(2);
    }
    impl std::ops::BitOr for KeyboardEnhancementFlags {
        type Output = Self;
        fn bitor(self, o: Self) -> Self { KeyboardEnhancementFlags(self.0 | o.0) }
    }
    pub struct PushKeyboardEnhancementFlags(pub KeyboardEnhancementFlags);
    pub struct PopKeyboardEnhancementFlags;
    pub struct EnableBracketedPaste;
    pub struct DisableBracketedPaste;
    impl Command for PushKeyboardEnhancementFlags { fn ansi(&self) -> &'static str { "" } }
    impl Command for PopKeyboardEnhancementFlags { fn ansi(&self) -> &'static str { "" } }
    impl Command for EnableBracketedPaste { fn ansi(&self) -> &'static str { "" } }
    impl Command for DisableBracketedPaste { fn ansi(&self) -> &'static str { "" } }
}

use event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

thread_local! {
    /// An event `poll` saw, kept for the `read` that follows it.
    static PENDING: RefCell<Option<Event>> = const { RefCell::new(None) };
}

fn poll(timeout: Duration) -> io::Result<bool> {
    if PENDING.with(|p| p.borrow().is_some()) { return Ok(true); }
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    let ev = next(ms);
    let got = ev.is_some();
    PENDING.with(|p| *p.borrow_mut() = ev);
    Ok(got)
}

fn read() -> io::Result<Event> {
    if let Some(ev) = PENDING.with(|p| p.borrow_mut().take()) { return Ok(ev); }
    loop {
        if let Some(ev) = next(-1) { return Ok(ev); }
    }
}

/// One event from the page, or None when `ms` ran out first.
fn next(ms: i32) -> Option<Event> {
    // What was drawn must reach the page before the program waits.
    let _ = io::stdout().flush();
    let mut buf = vec![0u8; 65536];
    let n = unsafe { page_key(buf.as_mut_ptr(), buf.len() as u32, ms) };
    if n <= 0 { return None; }
    parse(&String::from_utf8_lossy(&buf[..(n as usize).min(buf.len())]))
}

/// An event as the page sends it: see the top of this file.
fn parse(msg: &str) -> Option<Event> {
    let mut chars = msg.chars();
    match chars.next()? {
        'r' => terminal::size().ok().map(|(c, r)| Event::Resize(c, r)),
        'p' => Some(Event::Paste(chars.as_str().to_string())),
        'k' => {
            let (mods, name) = chars.as_str().split_once(' ')?;
            let m: u8 = mods.parse().ok()?;
            let code = match name {
                "Enter" => KeyCode::Enter,
                "Escape" => KeyCode::Esc,
                "Tab" if m & 1 != 0 => KeyCode::BackTab,
                "Tab" => KeyCode::Tab,
                "Backspace" => KeyCode::Backspace,
                "Delete" => KeyCode::Delete,
                "Insert" => KeyCode::Insert,
                "ArrowUp" => KeyCode::Up,
                "ArrowDown" => KeyCode::Down,
                "ArrowLeft" => KeyCode::Left,
                "ArrowRight" => KeyCode::Right,
                "Home" => KeyCode::Home,
                "End" => KeyCode::End,
                "PageUp" => KeyCode::PageUp,
                "PageDown" => KeyCode::PageDown,
                f if f.len() > 1 && f.starts_with('F') => KeyCode::F(f[1..].parse().ok()?),
                c => {
                    let mut cs = c.chars();
                    let ch = cs.next()?;
                    if cs.next().is_some() { return None; }
                    KeyCode::Char(ch)
                }
            };
            Some(Event::Key(KeyEvent { code, modifiers: KeyModifiers(m & 7), kind: KeyEventKind::Press }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_s_events_read_as_crossterm_s() {
        let key = |m| match parse(m) { Some(Event::Key(k)) => (k.code, k.modifiers), e => panic!("{e:?}") };
        assert_eq!(key("k0 ArrowUp"), (KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(key("k4 k"), (KeyCode::Char('k'), KeyModifiers::CONTROL));
        assert_eq!(key("k1 Tab").0, KeyCode::BackTab);
        assert_eq!(key("k0 F12").0, KeyCode::F(12));
        assert_eq!(key("k0  ").0, KeyCode::Char(' '));
        assert_eq!(key("k0 æ").0, KeyCode::Char('æ'));
        assert_eq!(parse("pone\ntwo"), Some(Event::Paste("one\ntwo".into())));
        assert_eq!(parse("r"), Some(Event::Resize(80, 24)));
        assert_eq!(parse("k0 Unidentified"), None);
        assert_eq!(parse("x"), None);
    }
}
