//! Forwarding an interrupt to the process groups `bounded::run` started
//! (D-T3-13): being in their own groups, they do not see a terminal's Ctrl-C.
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::bounded::end_group;

const SLOTS: usize = 64;
static GROUPS: [AtomicI32; SLOTS] = [const { AtomicI32::new(0) }; SLOTS];
static WAKE_FD: AtomicI32 = AtomicI32::new(-1);
pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub fn register(group: i32) -> Option<usize> {
    (0..SLOTS).find(|&i| GROUPS[i].compare_exchange(0, group, Ordering::SeqCst, Ordering::SeqCst).is_ok())
}

pub fn unregister(slot: Option<usize>) {
    if let Some(i) = slot {
        GROUPS[i].store(0, Ordering::SeqCst);
    }
}

extern "C" fn on_signal(signal: libc::c_int) {
    let fd = WAKE_FD.load(Ordering::SeqCst);
    let byte = signal as u8;
    // SAFETY: write(2) is async-signal-safe; the byte is a local.
    unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
}

/// Once per process: on SIGINT, SIGTERM or SIGHUP, end every running group,
/// then die of the same signal.
pub fn forward_signals() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: fds is a two-element array, as pipe(2) requires; FD_CLOEXEC
        // keeps both ends out of every command trantor runs.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return;
        }
        for fd in fds {
            unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        }
        WAKE_FD.store(fds[1], Ordering::SeqCst);
        let read_fd = fds[0];
        std::thread::spawn(move || {
            let mut byte = 0u8;
            // SAFETY: reads one byte into a local.
            if unsafe { libc::read(read_fd, (&mut byte as *mut u8).cast(), 1) } != 1 {
                return;
            }
            INTERRUPTED.store(true, Ordering::SeqCst);
            let groups: Vec<i32> = GROUPS.iter().map(|g| g.load(Ordering::SeqCst)).filter(|g| *g != 0).collect();
            let enders: Vec<_> = groups.into_iter().map(|g| std::thread::spawn(move || end_group(g))).collect();
            for e in enders {
                e.join().ok();
            }
            let signal = libc::c_int::from(byte);
            // SAFETY: restore the default disposition and die of the signal,
            // so the parent sees what happened.
            unsafe {
                libc::signal(signal, libc::SIG_DFL);
                libc::raise(signal);
            }
        });
        for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            // A signal the caller ignored — `nohup`, a background job — stays
            // ignored, and so stays ignored for the commands trantor runs.
            // SAFETY: querying and setting dispositions; on_signal only calls write(2).
            unsafe {
                let previous = libc::signal(s, on_signal as *const () as libc::sighandler_t);
                if previous == libc::SIG_IGN {
                    libc::signal(s, libc::SIG_IGN);
                }
            }
        }
    });
}
