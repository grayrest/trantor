//! imview driver HOST (hand-authored; H7 fixture, plan 2026-09-09). The
//! imview-slice reactor driver talking to its service components through the
//! GENERATED `abi::services` shim (D-H7-7): it never names a component. What
//! it owns is the loop — draining, routing, waiting on wakes, assembling `Env`
//! — and its own core command arms; everything per-service comes from the
//! shim. P0 wrote the shim by hand; P1 generates it from the manifest.
#![allow(dead_code)]
use abi::services;
use abi::{Element, ElementTag, Env, Event, RocBox, RocList};
use core::ffi::c_void;
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

unsafe extern "C-unwind" {
    fn roc_im_init(env: Env) -> RocBox;
    fn roc_im_view(model: RocBox, env: Env) -> abi::AnonStruct74775b1f87f9c7ed;
    fn roc_im_cmds(model: RocBox) -> RocList<abi::Cmd>;
    fn roc_im_route(model: RocBox, event: Event) -> RocBox;
}

// ---- the wake courier: the shim calls `wake`, the loop receives -------------

struct Wake(u32, *mut c_void);
// SAFETY: the token is an opaque component-owned pointer; only the runtime
// thread ever dereferences it (through the component's `complete`).
unsafe impl Send for Wake {}

static WAKES: Mutex<Option<Sender<Wake>>> = Mutex::new(None);

/// The driver's text measure (D-H7-22): this headless driver has no font, so
/// a byte is a unit.
extern "C" fn measure(_ptr: *const u8, len: usize, _font: u16) -> i32 {
    len as i32
}

fn wake(component_id: u32, token: *mut c_void) {
    if let Some(tx) = WAKES.lock().unwrap().as_ref() {
        let _ = tx.send(Wake(component_id, token));
    }
}

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

/// The per-frame Env: the driver's fields plus every service's env block.
fn env() -> Env {
    Env { width: 80, tick: services::env_tick() }
}

/// Drain the outbox: service commands go through the shim, core ones are the
/// driver's own arms. Returns how many asynchronous completions were promised.
fn drain(mut model: RocBox, next_request: &mut u64) -> (RocBox, u64) {
    let host = abi::host();
    unsafe { abi::incref_box(model, 1) }; // exposed procs consume their box
    let cmds = unsafe { roc_im_cmds(model) };
    let mut promised = 0;
    for c in cmds.as_slice() {
        let mut c = *c; // Copy shell; the payload is moved out exactly once
        *next_request += 1;
        // The fixture's loop needs to know how many wakes to wait for; a real
        // driver just runs. Peek before the payload moves.
        if let abi::CmdTag::Tick = c.tag {
            let t = unsafe { c.borrow_payload_tick_unchecked() };
            if let abi::TickTag::Start = t.tag {
                promised += unsafe { t.borrow_payload_start_unchecked() }._2;
            }
        }
        match services::dispatch(&mut c, *next_request) {
            Some(answers) => {
                for (route_key, event) in answers {
                    unsafe { route_key.decref(host) };
                    model = unsafe { roc_im_route(model, event) };
                }
            }
            None => match c.tag {
                abi::CmdTag::Log => {
                    let s = unsafe { c.take_payload_log_unchecked() };
                    eprintln!("[log] {}", s.as_str());
                    unsafe { s.decref(host) };
                }
                other => unreachable!("core arm missing for {other:?}"),
            },
        }
    }
    unsafe { cmds.decref(host) }; // shallow: every payload was moved out above
    (model, promised)
}

fn run() -> i32 {
    let host = abi::host();
    let (tx, rx): (Sender<Wake>, Receiver<Wake>) = channel();
    *WAKES.lock().unwrap() = Some(tx);
    services::init(wake, measure, None); // no draw lists here
    let mut next_request = 0u64;
    let mut model = unsafe { roc_im_init(env()) };
    let (m, promised) = drain(model, &mut next_request);
    model = m;
    for _ in 0..promised {
        let Wake(id, token) = rx.recv().expect("wake channel closed");
        for (_request, route_key, event) in services::on_wake(id, token) {
            unsafe { route_key.decref(host) };
            model = unsafe { roc_im_route(model, event) };
        }
    }
    let view = unsafe { roc_im_view(model, env()) };
    println!("{}", render(&view.tree));
    0
}

/// The gate chain: components first (through the shim), then the driver's own.
fn gate(name: &str, argv: &[String]) -> i32 {
    if let Some(answer) = services::gate(name, argv) {
        // A hook may answer with text as well as a code (D-H7-40); this
        // fixture's hooks do not, so `out` is empty and only the code matters.
        if !answer.out.is_empty() {
            println!("{}", answer.out);
        }
        return answer.code;
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
