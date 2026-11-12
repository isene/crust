//! Wait for a folder to change instead of looking at it on a timer.
//!
//! A program that shows a folder wants to know when a file is added or
//! removed. Looking every two seconds wakes the processor 43,000 times
//! a day for nothing. With a [`Watch`] the kernel wakes the program
//! when the folder changes, and [`Input::getchr`](crate::Input::getchr)
//! returns `"WATCH"`.
//!
//! ```no_run
//! use crust::{Input, Watch};
//!
//! let mut watch = Watch::new();
//! loop {
//!     let dir = std::env::current_dir().unwrap();
//!     // With a watch the wait has no end; without one, look now and then.
//!     let watching = watch.as_mut().is_some_and(|w| w.dir(&dir));
//!     let wait = if watching { None } else { Some(2) };
//!     match Input::getchr(wait).as_deref() {
//!         Some("WATCH") => { watch.as_mut().unwrap().drain(); /* read the folder again */ }
//!         Some(_key) => {}
//!         None => { /* the old way: has the folder changed? */ }
//!     }
//! }
//! ```
//!
//! Linux only (inotify). Elsewhere [`Watch::new`] gives `None` and the
//! program keeps its timer.

use std::path::{Path, PathBuf};

/// One watched folder. The watch ends when this is dropped.
pub struct Watch {
    #[cfg(target_os = "linux")]
    fd: std::os::fd::OwnedFd,
    #[cfg(target_os = "linux")]
    wd: i32,
    path: PathBuf,
}

impl Watch {
    /// A watch with no folder yet. None where the system has no
    /// inotify, or when it has none left to give.
    #[cfg(target_os = "linux")]
    pub fn new() -> Option<Watch> {
        use std::os::fd::{AsRawFd, FromRawFd};
        // SAFETY: inotify_init1 gives a fresh descriptor or -1.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 { return None; }
        // SAFETY: nothing else owns the descriptor.
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
        crate::input::wake_on(fd.as_raw_fd());
        Some(Watch { fd, wd: -1, path: PathBuf::new() })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new() -> Option<Watch> { None }

    /// Watch this folder, and no longer the one before. Costs nothing
    /// when it is the folder already watched. False when the folder
    /// cannot be watched; the caller then looks on a timer.
    pub fn dir(&mut self, path: &Path) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            use std::os::unix::ffi::OsStrExt;
            if self.wd >= 0 && self.path == path { return true; }
            // SAFETY: plain inotify calls on a descriptor we own, with
            // a path that ends in a zero byte.
            unsafe {
                if self.wd >= 0 { libc::inotify_rm_watch(self.fd.as_raw_fd(), self.wd); }
                let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { self.wd = -1; return false };
                // What changes a listing: a file comes, goes, is
                // renamed, written or gets new rights, or the folder
                // itself goes.
                let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO
                    | libc::IN_CLOSE_WRITE | libc::IN_ATTRIB | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF;
                self.wd = libc::inotify_add_watch(self.fd.as_raw_fd(), c.as_ptr(), mask);
            }
            self.path = path.to_path_buf();
            self.drain();
            self.wd >= 0
        }
        #[cfg(not(target_os = "linux"))]
        { let _ = path; false }
    }

    /// The folder being watched.
    pub fn path(&self) -> &Path { &self.path }

    /// Read away what the kernel has reported. True when there was
    /// something. Call it after `"WATCH"`, or the next wait returns at
    /// once.
    pub fn drain(&mut self) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let mut any = false;
            let mut buf = [0u8; 4096];
            // SAFETY: the buffer is ours and as long as we say.
            while unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) } > 0 { any = true; }
            any
        }
        #[cfg(not(target_os = "linux"))]
        false
    }
}

#[cfg(target_os = "linux")]
impl Drop for Watch {
    fn drop(&mut self) {
        crate::input::wake_on(-1);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn readable(w: &Watch) -> bool {
        use std::os::fd::AsRawFd;
        let mut p = libc::pollfd { fd: w.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd.
        unsafe { libc::poll(&mut p, 1, 200) > 0 }
    }

    #[test]
    fn a_new_file_wakes_the_watch_and_a_drain_quiets_it() {
        let dir = std::env::temp_dir().join(format!("crust-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = Watch::new().unwrap();
        assert!(w.dir(&dir));
        assert!(!w.drain());
        std::fs::write(dir.join("a"), "x").unwrap();
        assert!(readable(&w));
        assert!(w.drain());
        assert!(!w.drain());
        // Another folder: the old one no longer wakes it.
        let other = dir.join("sub");
        std::fs::create_dir_all(&other).unwrap();
        w.drain();
        assert!(w.dir(&other));
        std::fs::write(dir.join("b"), "x").unwrap();
        assert!(!w.drain());
        std::fs::write(other.join("c"), "x").unwrap();
        assert!(w.drain());
        assert!(!w.dir(&dir.join("not-there")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
