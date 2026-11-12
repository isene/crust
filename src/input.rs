//! Input handling - equivalent to rcurses Input module (getchr)

use crate::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Off: Alt+x reads as plain "x", as it always has. On: as "M-x".
static ALT_KEYS: AtomicBool = AtomicBool::new(false);

/// One more descriptor the key wait wakes up for, or -1: a
/// [`Watch`](crate::Watch) or a [`Wake`](crate::Wake) sets it. The wait
/// then returns "WATCH" or "WAKE".
#[cfg(target_os = "linux")]
static WAKE_FD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

#[cfg(target_os = "linux")]
pub(crate) fn wake_on(fd: i32, name: &str) {
    WAKE_IS_THREAD.store(name == "WAKE", Ordering::Relaxed);
    WAKE_FD.store(fd, Ordering::Relaxed);
}

/// Whether the extra descriptor is a thread's wake, not a folder watch.
#[cfg(target_os = "linux")]
static WAKE_IS_THREAD: AtomicBool = AtomicBool::new(false);

/// The name the wait returns when the extra descriptor woke it.
#[cfg(target_os = "linux")]
fn wake_name() -> String {
    if WAKE_IS_THREAD.load(Ordering::Relaxed) { "WAKE" } else { "WATCH" }.to_string()
}

/// True when the extra descriptor has something to read.
#[cfg(target_os = "linux")]
fn woken() -> bool {
    let fd = WAKE_FD.load(Ordering::Relaxed);
    if fd < 0 { return false; }
    let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    // SAFETY: one valid pollfd.
    unsafe { libc::poll(&mut p, 1, 0) > 0 && p.revents & libc::POLLIN != 0 }
}

/// What happened to a key, for readers that ask with `event_ms`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyState { Pressed, Repeated, Released }

pub struct Input;

impl Input {
    /// Report Alt+key as "M-x" ("M-1", "M-a") instead of the bare key.
    /// Off by default: a terminal sends Alt+x as Esc then x, so in an
    /// app where Esc is often followed at once by another key (a modal
    /// editor), that pair would start to read as an Alt key. Apps that
    /// bind Alt keys turn it on once at startup.
    pub fn report_alt(on: bool) {
        ALT_KEYS.store(on, Ordering::Relaxed);
    }

    /// Non-blocking peek: returns true if at least one input event is
    /// already queued, false otherwise. Used by image-displaying TUIs
    /// to skip expensive previews while the user is still hammering
    /// j/k on autorepeat — render the cheap state now, do the heavy
    /// graphics work after the burst ends.
    pub fn peek_pending() -> bool {
        Self::ready(Duration::from_millis(0))
    }

    /// Wait for input, at most `wait`. True when some is there.
    ///
    /// An error means there is no terminal to read: the program was
    /// started without one, or its terminal is gone. No key can ever
    /// come, and a caller that loops on "no key yet" would keep a core
    /// busy until someone kills it. So the program ends.
    fn ready(wait: Duration) -> bool {
        #[cfg(target_os = "linux")]
        if let Some(ready) = Self::ready_linux(wait, true) { return ready; }
        match event::poll(wait) {
            Ok(ready) => ready,
            #[cfg(not(target_os = "wasi"))]
            Err(_) => std::process::exit(1),
            #[cfg(target_os = "wasi")]
            Err(_) => false,
        }
    }

    /// The wait on Linux, when stdin is the terminal. None when it is
    /// not, and the caller waits the old way.
    ///
    /// A terminal that is gone (its window killed, tmux stopped) reads
    /// as zero bytes for ever, and crossterm reads it again and again
    /// without ever coming back: a full core until someone kills the
    /// program. So the wait itself is done here, and crossterm is only
    /// asked once the terminal is known to be alive. That costs two
    /// system calls more for each wait, and nothing while waiting.
    /// The bug is known there (crossterm issue 793, open since 2023,
    /// with fixes waiting as pull requests 1067, 1116 and 1165). When
    /// one is merged and crust builds on it, this can go.
    #[cfg(target_os = "linux")]
    fn ready_linux(wait: Duration, watch: bool) -> Option<bool> {
        use std::sync::OnceLock;
        use std::time::Instant;
        static TTY: OnceLock<bool> = OnceLock::new();
        if !*TTY.get_or_init(|| unsafe { libc::isatty(0) } == 1) { return None; }
        const GONE: libc::c_short = libc::POLLHUP | libc::POLLERR;
        // How many are ready, and whether the extra descriptor (a
        // folder watch) is one of them.
        let look = |ms: libc::c_int| {
            let extra = if watch { WAKE_FD.load(Ordering::Relaxed) } else { -1 };
            let mut fds = [libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: extra, events: libc::POLLIN, revents: 0 }];
            let n = unsafe { libc::poll(fds.as_mut_ptr(), if extra >= 0 { 2 } else { 1 }, ms) };
            if n > 0 && fds[0].revents & GONE != 0 { std::process::exit(1); }
            (n, n > 0 && fds[1].revents & libc::POLLIN != 0)
        };
        let end = Instant::now() + wait;
        loop {
            if look(0).1 { return Some(true); }
            // What crossterm has: a key, or a resize it heard of.
            match event::poll(Duration::ZERO) {
                Ok(true) => return Some(true),
                Ok(false) => {}
                Err(_) => std::process::exit(1),
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() { return Some(false); }
            // Sleep until a key, the end of the wait, or a signal (a
            // resize is one). Zero means the wait is over.
            if look(left.as_millis().min(i32::MAX as u128) as libc::c_int).0 == 0 { return Some(false); }
        }
    }

    /// Wait for a key with no end, for code that reads its keys from
    /// crossterm itself (a prompt). Like every wait here, it ends the
    /// program when the terminal is gone. A folder watch does not wake
    /// it: a prompt has no use for one.
    pub(crate) fn wait_key() {
        #[cfg(target_os = "linux")]
        while Self::ready_linux(Duration::from_secs(86400), false) == Some(false) {}
    }

    /// As [`getchr`](Self::getchr), with the wait in milliseconds. For a
    /// loop that is also draining something else, a stream of answer
    /// chunks say: block in the kernel until a key or the deadline, then
    /// look at the other thing. Idle programs keep using `getchr(None)`.
    pub fn getchr_ms(timeout_ms: u64) -> Option<String> {
        if !Self::ready(Duration::from_millis(timeout_ms)) { return None; }
        Self::getchr(Some(0))
    }

    /// As [`getchr_ms`](Self::getchr_ms), with what happened to the key:
    /// pressed, repeated or released. Releases and repeats only arrive
    /// when the terminal speaks the kitty keyboard protocol and
    /// [`Crust::enable_key_release`](crate::Crust::enable_key_release)
    /// was called; elsewhere every event is a press. A resize comes as
    /// "RESIZE", and after
    /// [`Crust::enable_focus_reports`](crate::Crust::enable_focus_reports)
    /// a focus change as "FOCUS_IN" or "FOCUS_OUT". A game needs this;
    /// `getchr` keeps skipping releases so no other app sees a key twice.
    pub fn event_ms(timeout_ms: u64) -> Option<(String, KeyState)> {
        if !Self::ready(Duration::from_millis(timeout_ms)) { return None; }
        #[cfg(target_os = "linux")]
        if woken() { return Some((wake_name(), KeyState::Pressed)); }
        match event::read() {
            Ok(Event::Key(KeyEvent { code, modifiers, kind, .. })) => {
                let state = match kind {
                    KeyEventKind::Release => KeyState::Released,
                    KeyEventKind::Repeat => KeyState::Repeated,
                    KeyEventKind::Press => KeyState::Pressed,
                };
                Some((Self::key_to_string(code, modifiers), state))
            }
            Ok(Event::Resize(_, _)) => Some(("RESIZE".to_string(), KeyState::Pressed)),
            // Only after `Crust::enable_focus_reports`.
            Ok(Event::FocusGained) => Some(("FOCUS_IN".to_string(), KeyState::Pressed)),
            Ok(Event::FocusLost) => Some(("FOCUS_OUT".to_string(), KeyState::Pressed)),
            _ => None,
        }
    }

    /// Read a single key event, returning a named string like rcurses.
    /// Returns None on timeout (if timeout_secs is Some).
    pub fn getchr(timeout_secs: Option<u64>) -> Option<String> {
        if !Self::ready(Duration::from_secs(timeout_secs.unwrap_or(86400))) {
            return None;
        }
        // A watched folder changed. The caller drains the watch.
        #[cfg(target_os = "linux")]
        if woken() { return Some(wake_name()); }

        let ev = match event::read() {
            Ok(ev) => ev,
            Err(_) => return None,
        };

        match ev {
            // A release is not a key press; only `event_ms` reports it.
            Event::Key(KeyEvent { kind: KeyEventKind::Release, .. }) => None,
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                Some(Self::key_to_string(code, modifiers))
            }
            Event::Resize(_, _) => Some("RESIZE".to_string()),
            // Only after `Crust::enable_focus_reports`.
            Event::FocusGained => Some("FOCUS_IN".to_string()),
            Event::FocusLost => Some("FOCUS_OUT".to_string()),
            // Bracketed paste — apps that called EnableBracketedPaste receive
            // the whole pasted payload as one event. Encode as "PASTE\x00<text>"
            // so apps can detect it via .starts_with("PASTE\x00").
            Event::Paste(s) => Some(format!("PASTE\x00{}", s)),
            _ => None,
        }
    }

    /// Convert a crossterm KeyEvent to rcurses-compatible string
    fn key_to_string(code: KeyCode, mods: KeyModifiers) -> String {
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);

        match code {
            KeyCode::Esc => "ESC".to_string(),
            // Shift+Enter (a new line in Claude Code) and Ctrl+Enter only
            // differ from Enter once the terminal is asked
            // (`enable_ctrl_shift_keys`).
            KeyCode::Enter if shift => "S-ENTER".to_string(),
            KeyCode::Enter if ctrl => "C-ENTER".to_string(),
            KeyCode::Enter => "ENTER".to_string(),
            KeyCode::Tab => {
                if shift { "S-TAB".to_string() } else { "TAB".to_string() }
            }
            KeyCode::BackTab => "S-TAB".to_string(),
            KeyCode::Backspace => {
                // Modifier on Backspace requires the terminal to send
                // distinct sequences (kitty keyboard protocol or
                // xterm modifyOtherKeys=2). Apps that want S-BACK
                // must enable crossterm's keyboard enhancement on
                // startup; without it, Shift+Backspace looks
                // identical to plain Backspace at the byte level.
                if ctrl { "WBACK".to_string() }
                else if shift { "S-BACK".to_string() }
                else { "BACK".to_string() }
            }
            KeyCode::Delete => {
                if ctrl { "C-DEL".to_string() } else { "DEL".to_string() }
            }
            KeyCode::Insert => {
                if ctrl { "C-INS".to_string() } else { "INS".to_string() }
            }
            KeyCode::Up => {
                if ctrl { "C-UP".to_string() }
                else if shift { "S-UP".to_string() }
                else { "UP".to_string() }
            }
            KeyCode::Down => {
                if ctrl { "C-DOWN".to_string() }
                else if shift { "S-DOWN".to_string() }
                else { "DOWN".to_string() }
            }
            KeyCode::Left => {
                if ctrl { "C-LEFT".to_string() }
                else if shift { "S-LEFT".to_string() }
                else { "LEFT".to_string() }
            }
            KeyCode::Right => {
                if ctrl { "C-RIGHT".to_string() }
                else if shift { "S-RIGHT".to_string() }
                else { "RIGHT".to_string() }
            }
            KeyCode::Home => {
                if ctrl { "C-HOME".to_string() } else { "HOME".to_string() }
            }
            KeyCode::End => {
                if ctrl { "C-END".to_string() } else { "END".to_string() }
            }
            KeyCode::PageUp => {
                if ctrl { "C-PgUP".to_string() } else { "PgUP".to_string() }
            }
            KeyCode::PageDown => {
                if ctrl { "C-PgDOWN".to_string() } else { "PgDOWN".to_string() }
            }
            KeyCode::F(n) => format!("F{}", n),
            KeyCode::Char(' ') if ctrl => "C-SPACE".to_string(),
            // Ctrl+Shift+letter only differs from Ctrl+letter once the
            // terminal is asked (`enable_ctrl_shift_keys`). crossterm then
            // reports the shift, or the shifted letter without it.
            KeyCode::Char(c) if ctrl && (shift || c.is_ascii_uppercase()) => {
                format!("C-S-{}", c.to_ascii_uppercase())
            }
            KeyCode::Char(c) if ctrl => {
                format!("C-{}", c.to_ascii_uppercase())
            }
            KeyCode::Char(c) if mods.contains(KeyModifiers::ALT) && ALT_KEYS.load(Ordering::Relaxed) => {
                format!("M-{}", c)
            }
            KeyCode::Char(c) => c.to_string(),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_keys_read_as_m_only_when_asked() {
        let alt1 = || Input::key_to_string(KeyCode::Char('1'), KeyModifiers::ALT);
        assert_eq!(alt1(), "1");
        Input::report_alt(true);
        assert_eq!(alt1(), "M-1");
        assert_eq!(Input::key_to_string(KeyCode::Char('1'), KeyModifiers::NONE), "1");
        Input::report_alt(false);
        assert_eq!(alt1(), "1");
    }

    #[test]
    fn ctrl_shift_letters_have_their_own_name() {
        let k = Input::key_to_string;
        assert_eq!(k(KeyCode::Char('w'), KeyModifiers::CONTROL), "C-W");
        assert_eq!(k(KeyCode::Char('w'), KeyModifiers::CONTROL | KeyModifiers::SHIFT), "C-S-W");
        assert_eq!(k(KeyCode::Char('W'), KeyModifiers::CONTROL), "C-S-W");
        assert_eq!(k(KeyCode::Enter, KeyModifiers::SHIFT), "S-ENTER");
        assert_eq!(k(KeyCode::Enter, KeyModifiers::CONTROL), "C-ENTER");
        assert_eq!(k(KeyCode::Enter, KeyModifiers::NONE), "ENTER");
    }
}
