//! Does blocking SIGWINCH everywhere but one dedicated thread keep a
//! process-directed SIGWINCH from interrupting blocking calls?
//!
//! Each case runs in a fresh process: `probe case <scenario> <call> <handler>`.
//! The signal is sent with kill(getpid()) — process-directed, like a real
//! resize — 1000ms into a 2000ms budget.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::io::FromRawFd;
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_millis(2000);
const INTERRUPT_AT: Duration = Duration::from_millis(1000);
const PIPE_WRITE_AT: Duration = Duration::from_millis(1500);
const HOLD: Duration = Duration::from_secs(10);
const PREEXISTING_REPEATS: usize = 10;

static DELIVERED: AtomicU64 = AtomicU64::new(0);
static HANDLED_ON: AtomicUsize = AtomicUsize::new(0);
static MAIN: AtomicUsize = AtomicUsize::new(0);
static SIGNAL_THREAD: AtomicUsize = AtomicUsize::new(0);
static WORKER: AtomicUsize = AtomicUsize::new(0);

fn this_thread() -> usize {
    unsafe { libc::pthread_self() as usize }
}

fn on_signal() {
    DELIVERED.fetch_add(1, Ordering::SeqCst);
    HANDLED_ON.store(this_thread(), Ordering::SeqCst);
}

extern "C" fn on_signal_c(_: libc::c_int) {
    on_signal();
}

fn install(handler: &str) {
    match handler {
        // The registry trantor-terminal uses: SA_RESTART.
        "restart" => unsafe {
            signal_hook_registry::register(libc::SIGWINCH, on_signal).expect("register");
        },
        "norestart" => unsafe {
            let mut action: libc::sigaction = core::mem::zeroed();
            action.sa_sigaction = on_signal_c as *const () as libc::sighandler_t;
            action.sa_flags = 0;
            libc::sigemptyset(&mut action.sa_mask);
            assert_eq!(libc::sigaction(libc::SIGWINCH, &action, core::ptr::null_mut()), 0);
        },
        other => panic!("handler {other}"),
    }
}

fn set_winch(how: libc::c_int) {
    unsafe {
        let mut set: libc::sigset_t = core::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGWINCH);
        assert_eq!(libc::pthread_sigmask(how, &set, core::ptr::null_mut()), 0);
    }
}

fn winch_blocked() -> bool {
    unsafe {
        let mut current: libc::sigset_t = core::mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, core::ptr::null(), &mut current);
        libc::sigismember(&current, libc::SIGWINCH) == 1
    }
}

/// A helper thread that must never take the signal, so the call's thread is
/// the only candidate besides the dedicated one.
fn helper(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        set_winch(libc::SIG_BLOCK);
        f()
    });
}

fn start_signal_thread() {
    let (ready, wait) = mpsc::channel();
    std::thread::spawn(move || {
        set_winch(libc::SIG_UNBLOCK);
        SIGNAL_THREAD.store(this_thread(), Ordering::SeqCst);
        ready.send(()).unwrap();
        loop {
            std::thread::park();
        }
    });
    wait.recv().unwrap();
}

fn arm_timer() {
    helper(|| {
        std::thread::sleep(INTERRUPT_AT);
        unsafe { libc::kill(libc::getpid(), libc::SIGWINCH) };
    });
}

/// ureq GET to a server that reads the request and never answers, with
/// http-host's per-request timeouts.
fn call_ureq() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    helper(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = s.read(&mut buf);
        std::thread::sleep(HOLD);
    });
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(BUDGET))
        .timeout_recv_response(Some(BUDGET))
        .timeout_recv_body(Some(BUDGET))
        .build()
        .into();
    arm_timer();
    match agent.get(&format!("http://127.0.0.1:{port}/")).call() {
        Ok(r) => format!("Ok({})", r.status()),
        Err(ureq::Error::Timeout(t)) => format!("Timeout({t:?})"),
        Err(ureq::Error::Io(e)) => format!("Io({:?})", e.kind()),
        Err(e) => format!("Err({e})"),
    }
}

/// std read on a TCP socket under SO_RCVTIMEO; the peer never sends.
fn call_tcpread() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (peer, _) = listener.accept().unwrap();
    client.set_read_timeout(Some(BUDGET)).unwrap();
    arm_timer();
    let mut buf = [0u8; 4];
    let result = match client.read(&mut buf) {
        Ok(n) => format!("Ok({n})"),
        Err(e) => format!("Io({:?})", e.kind()),
    };
    drop(peer);
    result
}

/// Plain blocking read on a pipe with no timeout (like trantor-cli's
/// Streams.read!); a byte arrives at 1500ms.
fn call_piperead() -> String {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let mut reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
    helper(move || {
        std::thread::sleep(PIPE_WRITE_AT);
        writer.write_all(b"x").unwrap();
        std::thread::sleep(HOLD);
    });
    arm_timer();
    let mut buf = [0u8; 1];
    match reader.read(&mut buf) {
        Ok(n) => format!("Ok({n})"),
        Err(e) => format!("Io({:?})", e.kind()),
    }
}

fn run_call(call: &str) -> String {
    match call {
        "ureq" => call_ureq(),
        "tcpread" => call_tcpread(),
        "piperead" => call_piperead(),
        other => panic!("call {other}"),
    }
}

fn label(thread: usize) -> &'static str {
    match thread {
        0 => "none",
        t if t == MAIN.load(Ordering::SeqCst) => "main",
        t if t == SIGNAL_THREAD.load(Ordering::SeqCst) => "signal-thread",
        t if t == WORKER.load(Ordering::SeqCst) => "worker",
        _ => "helper",
    }
}

fn case(scenario: &str, call: &str, handler: &str) {
    install(handler);
    MAIN.store(this_thread(), Ordering::SeqCst);
    let start = Instant::now();
    let outcome = match scenario {
        // Today: nothing masked; the call's thread is the only receiver.
        "baseline" => run_call(call),
        // Proposed: main blocks, then a dedicated thread takes the signal.
        "masked" => {
            set_winch(libc::SIG_BLOCK);
            start_signal_thread();
            run_call(call)
        }
        // A thread that existed before the mask was set does the call.
        "preexisting" => {
            let (go_tx, go_rx) = mpsc::channel::<String>();
            let (out_tx, out_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            std::thread::spawn(move || {
                WORKER.store(this_thread(), Ordering::SeqCst);
                ready_tx.send(()).unwrap();
                let call = go_rx.recv().unwrap();
                out_tx.send(run_call(&call)).unwrap();
            });
            ready_rx.recv().unwrap();
            set_winch(libc::SIG_BLOCK);
            start_signal_thread();
            go_tx.send(call.to_string()).unwrap();
            out_rx.recv().unwrap()
        }
        other => panic!("scenario {other}"),
    };
    let elapsed = start.elapsed().as_millis();
    // Let a signal that has not landed yet land before counting.
    std::thread::sleep(INTERRUPT_AT.saturating_sub(start.elapsed()) + Duration::from_millis(100));
    println!(
        "{scenario:<12} {call:<9} {handler:<10} {outcome:<22} {elapsed:>5}ms  delivered={} on={}",
        DELIVERED.load(Ordering::SeqCst),
        label(HANDLED_ON.load(Ordering::SeqCst)),
    );
}

/// Whether a child spawned from a masked main thread starts with SIGWINCH
/// blocked, through std's Command as-is and with a pre_exec unblock.
fn children(exe: &str) {
    use std::os::unix::process::CommandExt;
    install("restart");
    let report = |cmd: &mut Command| String::from_utf8(cmd.output().unwrap().stdout).unwrap().trim().to_string();
    println!("child of unmasked main, std Command:        {}", report(Command::new(exe).arg("report")));
    set_winch(libc::SIG_BLOCK);
    start_signal_thread();
    println!("child of masked main, std Command:          {}", report(Command::new(exe).arg("report")));
    let mut unblocking = Command::new(exe);
    unblocking.arg("report");
    unsafe {
        unblocking.pre_exec(|| {
            set_winch(libc::SIG_UNBLOCK);
            Ok(())
        });
    }
    println!("child of masked main, pre_exec unblock:     {}", report(&mut unblocking));
    // The shell's view, as a program like vim would inherit it.
    let shell = if cfg!(target_os = "linux") { "grep SigBlk /proc/self/status" } else { "echo no-proc-on-macos" };
    println!("masked main, sh sees:                       {}", report(Command::new("/bin/sh").args(["-c", shell])));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let exe = args[0].clone();
    match args.get(1).map(String::as_str) {
        Some("case") => case(&args[2], &args[3], &args[4]),
        Some("report") => println!("{}", if winch_blocked() { "SIGWINCH blocked" } else { "SIGWINCH unblocked" }),
        Some("children") => children(&exe),
        _ => {
            println!("{} ({}), signal at {INTERRUPT_AT:?} of {BUDGET:?}", std::env::consts::OS, std::env::consts::ARCH);
            for handler in ["restart", "norestart"] {
                for call in ["ureq", "tcpread", "piperead"] {
                    for scenario in ["baseline", "masked"] {
                        let _ = Command::new(&exe).args(["case", scenario, call, handler]).status();
                    }
                    for _ in 0..PREEXISTING_REPEATS {
                        let _ = Command::new(&exe).args(["case", "preexisting", call, handler]).status();
                    }
                }
            }
            let _ = Command::new(&exe).arg("children").status();
        }
    }
}
