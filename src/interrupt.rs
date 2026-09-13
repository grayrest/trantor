//! Forwarding an interrupt to the process groups `bounded::run` started
//! (D-T3-13): being in their own groups, they do not see a terminal's Ctrl-C.
//! A second interrupt SIGKILLs them at once; a nested `trantor test` killed
//! that way cannot end its own children, which are then left running — the
//! price of "kill it now".
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
            let signal = libc::c_int::from(byte);
            let groups: Vec<i32> = GROUPS.iter().map(|g| g.load(Ordering::SeqCst)).filter(|g| *g != 0).collect();
            // A second signal during the grace period kills them at once.
            let now: Vec<i32> = groups.clone();
            std::thread::spawn(move || {
                let mut again = 0u8;
                // SAFETY: reads one byte into a local.
                if unsafe { libc::read(read_fd, (&mut again as *mut u8).cast(), 1) } == 1 {
                    for g in &now {
                        // SAFETY: a negative pid addresses the process group.
                        unsafe { libc::kill(-g, libc::SIGKILL) };
                    }
                    die_of(signal);
                }
            });
            let enders: Vec<_> = groups.into_iter().map(|g| std::thread::spawn(move || end_group(g))).collect();
            note("trantor: interrupted — ending the running commands (interrupt again to kill them now)");
            for e in enders {
                e.join().ok();
            }
            die_of(signal);
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

/// A note on stderr that cannot stop the forwarding: stderr may be a closed
/// pipe (SIGPIPE would kill trantor before its commands) or a terminal that
/// just hung up (`eprintln!` would panic this thread and leave trantor waiting
/// forever). The groups are already being ended when it is written.
fn note(text: &str) {
    use std::io::Write;
    // SAFETY: trantor is exiting; a later write to a closed pipe must fail, not kill.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
    let _ = writeln!(std::io::stderr(), "{text}");
}

/// Restore the default disposition and die of the signal, so the parent sees
/// what happened.
fn die_of(signal: libc::c_int) {
    // SAFETY: changing a disposition and raising; no memory involved.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}
