//! Subprocesses `trantor test` runs, bounded (T3b). Each runs in its own
//! process group with its output going to files, not pipes: a test script that
//! leaves a peer server running used to hold the pipe open and block the run
//! forever, and a hung app had no deadline at all. On exit — or at the deadline
//! — whatever is left of the group is killed.
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Seconds any one suite, build or run may take. `TRANTOR_TEST_TIMEOUT`
/// overrides it.
const DEFAULT_TIMEOUT_SECS: u64 = 900;
const POLL: Duration = Duration::from_millis(50);

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

pub fn timeout_secs() -> u64 {
    std::env::var("TRANTOR_TEST_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// Run `cmd` to completion or the deadline, output captured under `scratch`.
pub fn run(cmd: &mut Command, what: &str, scratch: &Path) -> Result<Ran, String> {
    static N: AtomicU64 = AtomicU64::new(0);
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
    let group = child.id();
    let limit = timeout_secs();
    let deadline = Instant::now() + Duration::from_secs(limit);
    let (status, timed_out_after) = loop {
        if let Some(s) = child.try_wait().map_err(|e| format!("{what}: wait: {e}"))? {
            break (Some(s), None);
        }
        if Instant::now() >= deadline {
            kill_group(group, "KILL");
            break (child.wait().ok(), Some(limit));
        }
        std::thread::sleep(POLL);
    };
    // Anything the command left behind — a server a script started — goes too.
    kill_group(group, "KILL");
    let read = |p: &Path| std::fs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    Ok(Ran { status, stdout: read(&out_path), stderr: read(&err_path), timed_out_after })
}

fn kill_group(group: u32, signal: &str) {
    let _ = Command::new("kill").args([&format!("-{signal}"), "--", &format!("-{group}")])
        .stdout(Stdio::null()).stderr(Stdio::null()).status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("trantor-bounded-{}", std::process::id()))
    }

    #[test]
    fn a_background_child_neither_blocks_the_run_nor_survives_it() {
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
        std::env::set_var("TRANTOR_TEST_TIMEOUT", "1");
        let ran = run(Command::new("sleep").arg("30"), "sleeper", &scratch()).unwrap();
        std::env::remove_var("TRANTOR_TEST_TIMEOUT");
        assert_eq!(ran.timed_out_after, Some(1));
        assert!(!ran.ok() && ran.failure("sleeper").contains("killed after 1 s"));
    }
}
