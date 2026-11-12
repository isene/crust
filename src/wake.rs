//! Let a thread wake the key wait, instead of asking it on a timer.
//!
//! A program with a worker thread wants to draw the result when it
//! comes. Asking "done yet?" every second wakes the processor 86,000
//! times a day, mostly for nothing. With a [`Wake`] the thread says so
//! itself, and [`Input::getchr`](crate::Input::getchr) returns `"WAKE"`.
//!
//! ```no_run
//! use crust::{Input, Wake};
//!
//! let mut wake = Wake::new();
//! let (tx, rx) = std::sync::mpsc::channel();
//! let waker = wake.as_ref().map(|w| w.waker());
//! std::thread::spawn(move || {
//!     tx.send("a result").ok();
//!     if let Some(w) = &waker { w.wake(); }
//! });
//! loop {
//!     // With a wake the wait has no end; without one, look now and then.
//!     let wait = if wake.is_some() { None } else { Some(1) };
//!     match Input::getchr(wait).as_deref() {
//!         Some("WAKE") => { wake.as_mut().unwrap().drain(); }
//!         Some(_key) => {}
//!         None => {}
//!     }
//!     while let Ok(_result) = rx.try_recv() { /* draw it */ }
//! }
//! ```
//!
//! Linux only (eventfd). Elsewhere [`Wake::new`] gives `None` and the
//! program keeps its timer. A program has one extra thing to wake for:
//! a `Wake` or a [`Watch`](crate::Watch), and the one made last wins.

#[cfg(target_os = "linux")]
use std::sync::Arc;

/// The program's end: it makes [`Waker`]s and reads the wake away.
pub struct Wake {
    #[cfg(target_os = "linux")]
    fd: Arc<std::os::fd::OwnedFd>,
}

/// A thread's end. Cheap to clone, one for each thread.
#[derive(Clone)]
pub struct Waker {
    #[cfg(target_os = "linux")]
    fd: Arc<std::os::fd::OwnedFd>,
}

impl Wake {
    /// None where the system has no eventfd, or has none left to give.
    #[cfg(target_os = "linux")]
    pub fn new() -> Option<Wake> {
        use std::os::fd::{AsRawFd, FromRawFd};
        // SAFETY: eventfd gives a fresh descriptor or -1.
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd < 0 { return None; }
        // SAFETY: nothing else owns the descriptor.
        let fd = Arc::new(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) });
        crate::input::wake_on(fd.as_raw_fd(), "WAKE");
        Some(Wake { fd })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new() -> Option<Wake> { None }

    /// A handle for a thread.
    pub fn waker(&self) -> Waker {
        Waker {
            #[cfg(target_os = "linux")]
            fd: self.fd.clone(),
        }
    }

    /// Read the wake away. True when there was one. Call it after
    /// `"WAKE"`, or the next wait returns at once.
    pub fn drain(&mut self) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let mut n = 0u64;
            // SAFETY: eight bytes into a u64 we own.
            unsafe { libc::read(self.fd.as_raw_fd(), (&mut n as *mut u64).cast(), 8) == 8 }
        }
        #[cfg(not(target_os = "linux"))]
        false
    }
}

impl Waker {
    /// Wake the key wait. Many calls before the program looks count as
    /// one.
    pub fn wake(&self) {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let one = 1u64;
            // SAFETY: eight bytes from a u64 we own.
            unsafe { libc::write(self.fd.as_raw_fd(), (&one as *const u64).cast(), 8) };
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Wake {
    fn drop(&mut self) {
        crate::input::wake_on(-1, "");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn a_thread_wakes_it_and_a_drain_quiets_it() {
        let mut w = Wake::new().unwrap();
        assert!(!w.drain());
        let waker = w.waker();
        std::thread::spawn(move || { waker.wake(); waker.wake(); }).join().unwrap();
        assert!(w.drain());
        assert!(!w.drain());
    }
}
