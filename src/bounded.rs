//! Subprocesses `trantor test` runs, bounded (T3b). Each runs in its own
//! process group with its output going to files, not pipes: a test script that
//! leaves a peer server running used to hold the pipe open and block the run
//! forever, and a hung app had no deadline at all.
//!
//! A group is ended with SIGTERM, then SIGKILL after a grace period — at the
//! deadline, after the command exits (a server it left behind), and when
//! `trantor test` itself is interrupted. The SIGTERM matters: a nested
//! `trantor test` receives it and ends its own groups the same way, which a
//! SIGKILL would not let it do, and its grace is shorter than its parent's so
//! it finishes first. Being in their own groups, the children do not see a
//! terminal's Ctrl-C, so on SIGINT, SIGTERM or SIGHUP — each unless the caller
//! ignored it — trantor sends SIGTERM to them, then dies of the signal it got.
//! A child that calls `setsid` leaves the group and is out of reach; a suite
//! that starts one owns stopping it.
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::interrupt::{forward_signals, register, unregister, INTERRUPTED};
use std::time::{Duration, Instant};

/// Seconds any one suite, build or run may take. `TRANTOR_TEST_TIMEOUT`
/// overrides it.
const DEFAULT_TIMEOUT_SECS: u64 = 900;
/// A week: past this the value is a mistake, not a limit.
const MAX_TIMEOUT_SECS: u64 = 7 * 24 * 3600;
const POLL: Duration = Duration::from_millis(50);
/// How long a top-level group has to exit after SIGTERM before it is killed;
/// each level of nesting gets `GRACE_STEP` less, so an inner `trantor test`
/// ends its own groups before its parent kills it.
const GRACE_TOP: Duration = Duration::from_secs(10);
const GRACE_STEP: Duration = Duration::from_secs(3);
const GRACE_MIN: Duration = Duration::from_secs(1);
/// How deep in nested `trantor test` runs this process is.
const DEPTH_VAR: &str = "TRANTOR_TEST_DEPTH";
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
    wait_if_interrupted();
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::fs::create_dir_all(scratch).map_err(|e| format!("create {}: {e}", scratch.display()))?;
    let (out_path, err_path) = (scratch.join(format!("run-{n}.out")), scratch.join(format!("run-{n}.err")));
    let file = |p: &Path| std::fs::File::create(p).map_err(|e| format!("create {}: {e}", p.display()));
    let mut child = cmd
        .env(DEPTH_VAR, (depth() + 1).to_string())
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
    // Anything the command left behind — a server a script started — goes
    // too. Not when interrupted: the forwarding thread is ending the group, and
    // a second SIGTERM from here reached a nested `trantor test` as a second
    // interrupt, which kills its children without their grace.
    if !INTERRUPTED.load(Ordering::SeqCst) {
        end_group(group);
    }
    unregister(slot);
    wait_if_interrupted();
    Ok(Ran { status, stdout: tail(&out_path), stderr: tail(&err_path), timed_out_after })
}

/// Whether `pid` names a live process (one we may not signal still counts).
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false };
    // SAFETY: signal 0 checks existence and permission; it delivers nothing.
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

/// Interrupted: the forwarding thread is ending the groups and will re-raise
/// the signal. Returning would report the killed command as a failure and exit
/// 1 before it does, and starting another command would outlive the snapshot
/// of groups it ends.
fn wait_if_interrupted() {
    while INTERRUPTED.load(Ordering::SeqCst) {
        std::thread::sleep(POLL);
    }
}

fn depth() -> u32 {
    std::env::var(DEPTH_VAR).ok().and_then(|d| d.parse().ok()).unwrap_or(0)
}

fn grace() -> Duration {
    GRACE_TOP.saturating_sub(GRACE_STEP * depth()).max(GRACE_MIN)
}

/// SIGTERM, then SIGKILL for whatever is still there after the grace period.
pub fn end_group(group: i32) {
    if !signal_group(group, libc::SIGTERM) {
        return;
    }
    let until = Instant::now() + grace();
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
    fn a_nested_run_has_less_grace_than_its_parent() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let top = grace();
        std::env::set_var(DEPTH_VAR, "1");
        let nested = grace();
        std::env::set_var(DEPTH_VAR, "50");
        let deep = grace();
        std::env::remove_var(DEPTH_VAR);
        assert!(nested < top && deep == GRACE_MIN, "{top:?} {nested:?} {deep:?}");
    }

    #[test]
    fn a_pid_that_does_not_exist_is_not_alive_and_launchd_is() {
        assert!(!alive(999_999_999));
        assert!(alive(1));
    }
}
