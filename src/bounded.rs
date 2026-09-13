//! Subprocesses `trantor test` runs, bounded (T3b). Each runs in its own
//! process group with its output going to files, not pipes: a test script that
//! leaves a peer server running used to hold the pipe open and block the run
//! forever, and a hung app had no deadline at all.
//!
//! A group is ended with SIGTERM, then SIGKILL after a grace period — at the
//! deadline, after the command exits (a server it left behind), and when
//! `trantor test` itself is interrupted. The SIGTERM matters: a nested
//! `trantor test` receives it and ends its own groups the same way, which a
//! SIGKILL would not let it do. Being in their own groups, the children do not
//! see a terminal's Ctrl-C, so trantor forwards SIGINT, SIGTERM and SIGHUP to
//! them before exiting. A child that calls `setsid` leaves the group and is out
//! of reach; a suite that starts one owns stopping it.
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Seconds any one suite, build or run may take. `TRANTOR_TEST_TIMEOUT`
/// overrides it.
const DEFAULT_TIMEOUT_SECS: u64 = 900;
/// A week: past this the value is a mistake, not a limit.
const MAX_TIMEOUT_SECS: u64 = 7 * 24 * 3600;
const POLL: Duration = Duration::from_millis(50);
/// How long a group has to exit after SIGTERM before it is killed.
const GRACE: Duration = Duration::from_secs(5);
/// The most of a command's stdout or stderr kept in memory: the end of it.
const OUTPUT_CAP: u64 = 16 * 1024 * 1024;

pub struct Ran {
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out_after: Option<u64>,
}

impl Ran {
    pub fn ok(&self) -> bool {
        self.timed_out_after.is_none() && self.status.is_some_and(|s| s.success())
    }

    /// Why it did not succeed, with everything it printed.
    pub fn failure(&self, what: &str) -> String {
        let why = match (self.timed_out_after, self.status) {
            (Some(secs), _) => format!("{what}: killed after {secs} s (TRANTOR_TEST_TIMEOUT)"),
            (None, Some(s)) => format!("{what}: exited {s}"),
            (None, None) => format!("{what}: did not exit"),
        };
        format!("{why}\n{}{}", self.stdout, self.stderr)
    }
}

pub fn timeout_secs() -> Result<u64, String> {
    let Ok(v) = std::env::var("TRANTOR_TEST_TIMEOUT") else { return Ok(DEFAULT_TIMEOUT_SECS) };
    match v.trim().parse::<u64>() {
        Ok(n) if (1..=MAX_TIMEOUT_SECS).contains(&n) => Ok(n),
        _ => Err(format!("TRANTOR_TEST_TIMEOUT={v:?}: expected whole seconds from 1 to {MAX_TIMEOUT_SECS}")),
    }
}

/// Run `cmd` to completion or the deadline, output captured under `scratch`.
pub fn run(cmd: &mut Command, what: &str, scratch: &Path) -> Result<Ran, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let limit = timeout_secs()?;
    forward_signals();
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::fs::create_dir_all(scratch).map_err(|e| format!("create {}: {e}", scratch.display()))?;
    let (out_path, err_path) = (scratch.join(format!("run-{n}.out")), scratch.join(format!("run-{n}.err")));
    let file = |p: &Path| std::fs::File::create(p).map_err(|e| format!("create {}: {e}", p.display()));
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(file(&out_path)?)
        .stderr(file(&err_path)?)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{what}: spawn: {e}"))?;
    let group = child.id() as i32;
    let slot = register(group);
    let deadline = Instant::now() + Duration::from_secs(limit);
    let (status, timed_out_after) = loop {
        if let Some(s) = child.try_wait().map_err(|e| format!("{what}: wait: {e}"))? {
            break (Some(s), None);
        }
        if Instant::now() >= deadline {
            end_group(group);
            break (child.wait().ok(), Some(limit));
        }
        std::thread::sleep(POLL);
    };
    // Anything the command left behind — a server a script started — goes too.
    end_group(group);
    unregister(slot);
    Ok(Ran { status, stdout: tail(&out_path), stderr: tail(&err_path), timed_out_after })
}

/// Whether `pid` names a live process (one we may not signal still counts).
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false };
    // SAFETY: signal 0 checks existence and permission; it delivers nothing.
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

/// SIGTERM, then SIGKILL for whatever is still there after the grace period.
fn end_group(group: i32) {
    if !signal_group(group, libc::SIGTERM) {
        return;
    }
    let until = Instant::now() + GRACE;
    while Instant::now() < until {
        if !signal_group(group, 0) {
            return;
        }
        std::thread::sleep(POLL);
    }
    signal_group(group, libc::SIGKILL);
}

/// False once the group has no members.
fn signal_group(group: i32, signal: i32) -> bool {
    // SAFETY: a negative pid addresses the process group; no memory involved.
    unsafe { libc::kill(-group, signal) == 0 }
}

fn tail(p: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(p) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let skipped = len.saturating_sub(OUTPUT_CAP);
    let mut bytes = Vec::new();
    if f.seek(SeekFrom::Start(skipped)).is_err() || f.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if skipped == 0 { text } else { format!("[the first {skipped} bytes of {} are omitted]\n{text}", p.display()) }
}

// ---- forwarding an interrupt to the groups --------------------------------

const SLOTS: usize = 64;
static GROUPS: [AtomicI32; SLOTS] = [const { AtomicI32::new(0) }; SLOTS];
static WAKE_FD: AtomicI32 = AtomicI32::new(-1);

fn register(group: i32) -> Option<usize> {
    (0..SLOTS).find(|&i| GROUPS[i].compare_exchange(0, group, Ordering::SeqCst, Ordering::SeqCst).is_ok())
}

fn unregister(slot: Option<usize>) {
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
fn forward_signals() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: fds is a two-element array, as pipe(2) requires.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return;
        }
        WAKE_FD.store(fds[1], Ordering::SeqCst);
        let read_fd = fds[0];
        std::thread::spawn(move || {
            let mut byte = 0u8;
            // SAFETY: reads one byte into a local.
            if unsafe { libc::read(read_fd, (&mut byte as *mut u8).cast(), 1) } != 1 {
                return;
            }
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
            // SAFETY: on_signal only calls write(2).
            unsafe { libc::signal(s, on_signal as *const () as libc::sighandler_t) };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests that read or set TRANTOR_TEST_TIMEOUT run one at a time.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn scratch() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("trantor-bounded-{}", std::process::id()))
    }

    #[test]
    fn a_background_child_neither_blocks_the_run_nor_survives_it() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let marker = scratch().join("alive");
        let script = format!("(sleep 30; touch {}) & echo started", marker.display());
        let start = Instant::now();
        let ran = run(Command::new("sh").args(["-c", &script]), "bg", &scratch()).unwrap();
        assert!(ran.ok() && ran.stdout.contains("started"));
        assert!(start.elapsed() < Duration::from_secs(10), "the run waited for the background child");
        std::thread::sleep(Duration::from_millis(300));
        assert!(!marker.exists());
    }

    #[test]
    fn a_command_past_its_deadline_is_killed_and_reported() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("TRANTOR_TEST_TIMEOUT", "1");
        let ran = run(Command::new("sleep").arg("30"), "sleeper", &scratch());
        std::env::remove_var("TRANTOR_TEST_TIMEOUT");
        let ran = ran.unwrap();
        assert_eq!(ran.timed_out_after, Some(1));
        assert!(!ran.ok() && ran.failure("sleeper").contains("killed after 1 s"));
    }

    #[test]
    fn a_timeout_that_is_zero_huge_or_not_a_number_is_refused() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        for bad in ["0", "18446744073709551615", "15m", ""] {
            std::env::set_var("TRANTOR_TEST_TIMEOUT", bad);
            let e = timeout_secs();
            std::env::remove_var("TRANTOR_TEST_TIMEOUT");
            assert!(e.is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn only_the_end_of_a_huge_output_is_kept() {
        let p = scratch().join("huge.out");
        std::fs::create_dir_all(scratch()).unwrap();
        let f = std::fs::File::create(&p).unwrap();
        f.set_len(OUTPUT_CAP + 10).unwrap();
        let t = tail(&p);
        assert!(t.starts_with("[the first 10 bytes") && t.len() < (OUTPUT_CAP + 200) as usize);
    }

    #[test]
    fn a_pid_that_does_not_exist_is_not_alive_and_launchd_is() {
        assert!(!alive(999_999_999));
        assert!(alive(1));
    }
}
