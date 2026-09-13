//! Does moving SIGCONT (and SIGTSTP) to the receiver thread keep a stop and
//! resume out of blocking calls on the app's thread, as D-K1-29 did for
//! SIGWINCH?
//!
//! Each case runs in a fresh process: `stop case <setup> <call>`. A separate
//! `sh` sends the stop 1000ms into a 2000ms budget and SIGCONT 300ms later,
//! because a stopped process cannot resume itself. Handlers are installed the
//! way trantor-terminal's guard installs them: through signal-hook-registry
//! (`SA_RESTART`), SIGTSTP emulating its default (`raise(SIGSTOP)`).
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::io::FromRawFd;
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_millis(2000);
const PIPE_WRITE_AT: Duration = Duration::from_millis(1500);
const HOLD: Duration = Duration::from_secs(10);

static TSTP_ON: AtomicUsize = AtomicUsize::new(0);
static CONT_ON: AtomicUsize = AtomicUsize::new(0);
static CONT_COUNT: AtomicU64 = AtomicU64::new(0);
static MAIN: AtomicUsize = AtomicUsize::new(0);
static RECEIVER: AtomicUsize = AtomicUsize::new(0);

fn this_thread() -> usize {
    unsafe { libc::pthread_self() as usize }
}

fn set_of(signals: &[libc::c_int]) -> libc::sigset_t {
    unsafe {
        let mut set: libc::sigset_t = core::mem::zeroed();
        libc::sigemptyset(&mut set);
        for s in signals {
            libc::sigaddset(&mut set, *s);
        }
        set
    }
}

fn mask(how: libc::c_int, signals: &[libc::c_int]) {
    let set = set_of(signals);
    assert_eq!(unsafe { libc::pthread_sigmask(how, &set, core::ptr::null_mut()) }, 0);
}

fn install_guard_like() {
    unsafe {
        signal_hook_registry::register_sigaction(libc::SIGTSTP, |_| {
            TSTP_ON.store(this_thread(), Ordering::SeqCst);
            let _ = signal_hook::low_level::emulate_default_handler(libc::SIGTSTP);
        })
        .expect("SIGTSTP");
        signal_hook_registry::register_sigaction(libc::SIGCONT, |_| {
            CONT_COUNT.fetch_add(1, Ordering::SeqCst);
            CONT_ON.store(this_thread(), Ordering::SeqCst);
        })
        .expect("SIGCONT");
        signal_hook_registry::register_sigaction(libc::SIGWINCH, |_| {}).expect("SIGWINCH");
    }
}

/// D-K1-29's receiver, taking `signals` and nothing else; the calling thread
/// then blocks them.
fn move_to_receiver(signals: &'static [libc::c_int]) {
    let (ready, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let all_but = unsafe {
            let mut set: libc::sigset_t = core::mem::zeroed();
            libc::sigfillset(&mut set);
            for s in signals {
                libc::sigdelset(&mut set, *s);
            }
            set
        };
        assert_eq!(unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &all_but, core::ptr::null_mut()) }, 0);
        RECEIVER.store(this_thread(), Ordering::SeqCst);
        ready.send(()).unwrap();
        loop {
            std::thread::park();
        }
    });
    wait.recv().unwrap();
    mask(libc::SIG_BLOCK, signals);
}

fn helper(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        mask(libc::SIG_BLOCK, &[libc::SIGTSTP, libc::SIGCONT, libc::SIGWINCH]);
        f()
    });
}

fn arm_sender(stop: &str) {
    let pid = std::process::id();
    Command::new("/bin/sh")
        .args(["-c", &format!("sleep 1; kill -{stop} {pid}; sleep 0.3; kill -CONT {pid}")])
        .spawn()
        .expect("sender");
}

fn call_ureq(stop: &str) -> String {
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
    arm_sender(stop);
    match agent.get(&format!("http://127.0.0.1:{port}/")).call() {
        Ok(r) => format!("Ok({})", r.status()),
        Err(ureq::Error::Timeout(t)) => format!("Timeout({t:?})"),
        Err(ureq::Error::Io(e)) => format!("Io({:?})", e.kind()),
        Err(e) => format!("Err({e})"),
    }
}

fn call_tcpread(stop: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (peer, _) = listener.accept().unwrap();
    client.set_read_timeout(Some(BUDGET)).unwrap();
    arm_sender(stop);
    let mut buf = [0u8; 4];
    let result = match client.read(&mut buf) {
        Ok(n) => format!("Ok({n})"),
        Err(e) => format!("Io({:?})", e.kind()),
    };
    drop(peer);
    result
}

fn call_piperead(stop: &str) -> String {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let mut reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
    helper(move || {
        std::thread::sleep(PIPE_WRITE_AT);
        writer.write_all(b"x").unwrap();
        std::thread::sleep(HOLD);
    });
    arm_sender(stop);
    let mut buf = [0u8; 1];
    match reader.read(&mut buf) {
        Ok(n) => format!("Ok({n})"),
        Err(e) => format!("Io({:?})", e.kind()),
    }
}

fn label(thread: usize) -> &'static str {
    match thread {
        0 => "-",
        t if t == MAIN.load(Ordering::SeqCst) => "main",
        t if t == RECEIVER.load(Ordering::SeqCst) => "receiver",
        _ => "other",
    }
}

fn case(setup: &str, call: &str) {
    MAIN.store(this_thread(), Ordering::SeqCst);
    let stop = match setup {
        // No handlers at all: what the kernel does on its own.
        "no-handlers" => "STOP",
        // As D-K1-29 built it: SIGWINCH moved, SIGTSTP and SIGCONT on main.
        "winch" => {
            install_guard_like();
            move_to_receiver(&[libc::SIGWINCH]);
            "TSTP"
        }
        "winch+cont" => {
            install_guard_like();
            move_to_receiver(&[libc::SIGWINCH, libc::SIGCONT]);
            "TSTP"
        }
        "winch+cont+tstp" => {
            install_guard_like();
            move_to_receiver(&[libc::SIGWINCH, libc::SIGCONT, libc::SIGTSTP]);
            "TSTP"
        }
        other => panic!("setup {other}"),
    };
    let start = Instant::now();
    let outcome = match call {
        "ureq" => call_ureq(stop),
        "tcpread" => call_tcpread(stop),
        "piperead" => call_piperead(stop),
        other => panic!("call {other}"),
    };
    let elapsed = start.elapsed().as_millis();
    // Let the sender finish before reading where the handlers ran.
    std::thread::sleep(Duration::from_millis(1500).saturating_sub(start.elapsed()));
    println!(
        "{setup:<16} {call:<9} {outcome:<22} {elapsed:>5}ms  tstp-on={} cont-on={} conts={}",
        label(TSTP_ON.load(Ordering::SeqCst)),
        label(CONT_ON.load(Ordering::SeqCst)),
        CONT_COUNT.load(Ordering::SeqCst),
    );
}

const REPEATS: usize = 3;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("case") {
        return case(&args[2], &args[3]);
    }
    println!("{} ({}), stop at 1s, SIGCONT at 1.3s, of a 2s budget", std::env::consts::OS, std::env::consts::ARCH);
    for call in ["ureq", "tcpread", "piperead"] {
        for setup in ["no-handlers", "winch", "winch+cont", "winch+cont+tstp"] {
            for _ in 0..REPEATS {
                let _ = Command::new(&args[0]).args(["case", setup, call]).status();
            }
        }
    }
}
