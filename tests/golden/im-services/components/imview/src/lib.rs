//! imview driver HOST (hand-authored; P0 spike of plan 2026-09-09 H7). It is
//! the imview-slice reactor driver plus, written BY HAND, everything P1's
//! generated `abi/src/services.rs` will emit: the `HostCtx` handed to each
//! component, the per-component dispatch of wrapper commands, the wake courier
//! (any thread -> `wake(id, token)` -> runtime-thread `complete`), the env-block
//! assembly, and the gate chain (components are asked before the driver's own
//! arms). What it measures is listed in `verify.sh`.
#![allow(dead_code)]
use abi::{Element, ElementTag, Env, Event, EventPayload, EventTag, RocBox, RocList, RocStr, TickEnv};
use core::ffi::c_void;
use core::mem::ManuallyDrop;
use core::ptr;
use hematite_abi as abi;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

#[unsafe(no_mangle)] pub extern "C" fn roc_alloc(l: usize, a: usize) -> *mut c_void { abi::DefaultAllocators::roc_alloc(ptr::null_mut(), l, a) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_dealloc(p: *mut c_void, a: usize) { abi::DefaultAllocators::roc_dealloc(ptr::null_mut(), p, a) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_realloc(p: *mut c_void, n: usize, a: usize) -> *mut c_void { abi::DefaultAllocators::roc_realloc(ptr::null_mut(), p, n, a) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_dbg(b: *const u8, n: usize) { eprintln!("[dbg] {}", String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(b, n) })); }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_expect_failed(b: *const u8, n: usize) { eprintln!("[expect] {}", String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(b, n) })); }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_crashed(b: *const u8, n: usize) { eprintln!("[CRASH] {}", String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(b, n) })); std::process::exit(1); }

// ---- the service contract (P1 generates this into the abi crate) ----------

/// D8's `HostCtx`: what the driver hands each component at `init`.
#[repr(C)]
pub struct HostCtx {
    pub component_id: u32,
    /// Callable from any thread. The token is component-owned (D9).
    pub wake: extern "C" fn(u32, *mut c_void),
}

/// A synchronous answer to a wrapper command: the component's own event type.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EchoAnswer { pub route_key: RocStr, pub event: abi::EchoEvent }
/// `TickEvent` is a single-variant union, so glue unwraps it to its payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TickAnswer { pub route_key: RocStr, pub event: u64 }
/// What `complete` returns for a token the component woke us with.
#[repr(C)]
pub struct TickCompletion { pub request: u64, pub route_key: RocStr, pub event: u64 }

const ECHO_ID: u32 = 1;
const TICK_ID: u32 = 2;

unsafe extern "C-unwind" {
    fn hematite__svc_echo__init(ctx: *const HostCtx);
    fn hematite__svc_echo__cmd(request: u64, route_key: RocStr, cmd: abi::Echo) -> RocList<EchoAnswer>;
    fn hematite__svc_echo__gate(name: RocStr, argv: RocList<RocStr>) -> i32;
    fn hematite__svc_tick__init(ctx: *const HostCtx);
    fn hematite__svc_tick__cmd(request: u64, route_key: RocStr, cmd: abi::Tick) -> RocList<TickAnswer>;
    fn hematite__svc_tick__complete(token: *mut c_void) -> TickCompletion;
    fn hematite__svc_tick__env() -> TickEnv;
    fn hematite__svc_tick__gate(name: RocStr, argv: RocList<RocStr>) -> i32;
}

unsafe extern "C-unwind" {
    fn roc_im_init(env: Env) -> RocBox;
    fn roc_im_view(model: RocBox, env: Env) -> abi::AnonStruct74775b1f87f9c7ed;
    fn roc_im_cmds(model: RocBox) -> RocList<abi::Cmd>;
    fn roc_im_route(model: RocBox, event: Event) -> RocBox;
}

// ---- the wake courier -------------------------------------------------------

struct Wake(u32, *mut c_void);
// SAFETY: the token is an opaque component-owned pointer; only the runtime
// thread ever dereferences it (through the component's `complete`).
unsafe impl Send for Wake {}

static WAKES: Mutex<Option<Sender<Wake>>> = Mutex::new(None);

extern "C" fn wake(component_id: u32, token: *mut c_void) {
    if let Some(tx) = WAKES.lock().unwrap().as_ref() {
        let _ = tx.send(Wake(component_id, token));
    }
}

static ECHO_CTX: HostCtx = HostCtx { component_id: ECHO_ID, wake };
static TICK_CTX: HostCtx = HostCtx { component_id: TICK_ID, wake };

// ---- the driver -------------------------------------------------------------

fn render(el: &Element) -> String {
    match el.tag {
        ElementTag::Text => unsafe { el.borrow_payload_text_unchecked().as_str().to_string() },
        ElementTag::Row => {
            let kids = unsafe { el.borrow_payload_row_unchecked() };
            let inner: Vec<String> = kids.as_slice().iter().map(render).collect();
            format!("[{}]", inner.join(" | "))
        }
    }
}

fn env() -> Env {
    // The env-block assembly: each env component contributes its block.
    Env { width: 80, tick: unsafe { hematite__svc_tick__env() } }
}

fn route(model: RocBox, event: Event) -> RocBox {
    unsafe { roc_im_route(model, event) }
}

/// Drain the outbox and dispatch every wrapper command to its component,
/// routing synchronous answers back at once. Returns how many asynchronous
/// completions were promised (the driver waits for that many wakes).
fn drain(mut model: RocBox, next_request: &mut u64) -> (RocBox, u64) {
    let host = abi::host();
    unsafe { abi::incref_box(model, 1) }; // exposed procs consume their box
    let cmds = unsafe { roc_im_cmds(model) };
    let mut promised = 0;
    for c in cmds.as_slice() {
        let mut c = *c; // Copy shell; the payload is moved out exactly once below
        *next_request += 1;
        match c.tag {
            abi::CmdTag::Log => {
                let s = unsafe { c.take_payload_log_unchecked() };
                eprintln!("[log] {}", s.as_str());
                unsafe { s.decref(host) };
            }
            abi::CmdTag::Echo => {
                let payload = unsafe { c.take_payload_echo_unchecked() };
                let key = RocStr::from_str("echo-key", host);
                let answers = unsafe { hematite__svc_echo__cmd(*next_request, key, payload) };
                for a in answers.as_slice() {
                    unsafe { a.route_key.decref(host) };
                    let event = Event { payload: EventPayload { echo: ManuallyDrop::new(a.event) }, tag: EventTag::Echo };
                    model = route(model, event);
                }
                unsafe { answers.decref(host) };
            }
            abi::CmdTag::Tick => {
                let payload = unsafe { c.take_payload_tick_unchecked() };
                if let abi::TickTag::Start = payload.tag {
                    promised += unsafe { payload.borrow_payload_start_unchecked() }._2;
                }
                let key = RocStr::from_str("tick-key", host);
                let answers = unsafe { hematite__svc_tick__cmd(*next_request, key, payload) };
                for a in answers.as_slice() {
                    unsafe { a.route_key.decref(host) };
                    let event = Event { payload: EventPayload { tick: ManuallyDrop::new(a.event) }, tag: EventTag::Tick };
                    model = route(model, event);
                }
                unsafe { answers.decref(host) };
            }
        }
    }
    unsafe { cmds.decref(host) }; // shallow: every payload was moved out above
    (model, promised)
}

fn run() -> i32 {
    let host = abi::host();
    let (tx, rx): (Sender<Wake>, Receiver<Wake>) = channel();
    *WAKES.lock().unwrap() = Some(tx);
    unsafe {
        hematite__svc_echo__init(&ECHO_CTX);
        hematite__svc_tick__init(&TICK_CTX);
    }
    let mut next_request = 0u64;
    let mut model = unsafe { roc_im_init(env()) };
    let (m, promised) = drain(model, &mut next_request);
    model = m;
    // Wait for every promised asynchronous completion, routing each on THIS
    // thread (the Roc value is built inside the component's `complete`).
    for _ in 0..promised {
        let Wake(id, token) = rx.recv().expect("wake channel closed");
        assert_eq!(id, TICK_ID, "only the tick component wakes in this spike");
        let done = unsafe { hematite__svc_tick__complete(token) };
        unsafe { done.route_key.decref(host) };
        let event = Event { payload: EventPayload { tick: ManuallyDrop::new(done.event) }, tag: EventTag::Tick };
        model = route(model, event);
    }
    let view = unsafe { roc_im_view(model, env()) };
    println!("{}", render(&view.tree));
    0
}

/// The gate chain: components first (−1 = not mine), then the driver's own.
fn gate(name: &str, argv: &[String]) -> i32 {
    let host = abi::host();
    let args: Vec<RocStr> = argv.iter().map(|a| RocStr::from_str(a, host)).collect();
    let argv_list = unsafe { RocList::from_slice(&args, host) };
    for g in [hematite__svc_echo__gate as unsafe extern "C-unwind" fn(RocStr, RocList<RocStr>) -> i32, hematite__svc_tick__gate] {
        let rc = unsafe { g(RocStr::from_str(name, host), argv_list) };
        if rc != -1 {
            return rc;
        }
    }
    match name {
        "driver-gate" => 3,
        _ => {
            eprintln!("unknown gate {name:?}");
            64
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let args: Vec<String> = (0..argc as usize)
        .map(|i| unsafe { std::ffi::CStr::from_ptr(*argv.add(i) as *const _) }.to_string_lossy().into_owned())
        .collect();
    let outcome = std::panic::catch_unwind(|| match args.get(1) {
        Some(name) => gate(name, &args),
        None => run(),
    });
    match outcome {
        Ok(rc) => rc,
        Err(_) => { eprintln!("[hematite] driver caught a panic"); 70 }
    }
}
