//! `svc-tick` — an ASYNCHRONOUS service component (roc-solid's `dbx`/`net`
//! shape): `Tick.Start(_, _, n)` spawns a thread that wakes the driver `n`
//! times through `HostCtx.wake`; the driver calls `complete(token)` on ITS
//! thread and only there is a Roc value built. It also owns an env block
//! (`TickEnv`, roc-solid's `audio` shape), read once per frame.
//!
//! Ownership (D9 / D-H7-11): the token is a `Box` this crate allocates and
//! this crate frees inside `complete`. Nothing Rust-allocated crosses.
use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use hematite_abi as abi;
use abi::services::{self, HostCtx};
use abi::{RocList, RocStr, Tick, TickEnv, TickEvent, TickEventPayload, TickEventTag, TickTag};

/// The contract types come from the generated shim (D-H7-7).
type Answer = services::Answer<TickEvent>;
type Completion = services::Completion<TickEvent>;

/// Component-owned; travels as the opaque wake token.
struct Pending { request: u64, route_key: String, n: u64 }

static CTX: AtomicPtr<HostCtx> = AtomicPtr::new(core::ptr::null_mut());
static TICKS: AtomicU64 = AtomicU64::new(0);

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_tick__init(ctx: *const HostCtx) {
    CTX.store(ctx as *mut HostCtx, Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_tick__cmd(request: u64, cmd: Tick) -> RocList<Answer> {
    let host = abi::host();
    if let TickTag::Start = cmd.tag {
        let start = unsafe { cmd.borrow_payload_start_unchecked() };
        let n = start._2;
        let key = start._1.as_str().to_string(); // the service's own route key (D-H7-17)
        let ctx = CTX.load(Ordering::Acquire);
        assert!(!ctx.is_null(), "svc-tick: cmd before init");
        // SAFETY: the driver's HostCtx is a static that outlives every call.
        let (id, wake) = unsafe { ((*ctx).component_id, (*ctx).wake) };
        std::thread::spawn(move || {
            for i in 1..=n {
                std::thread::sleep(std::time::Duration::from_millis(5));
                let token = Box::into_raw(Box::new(Pending { request, route_key: key.clone(), n: i }));
                wake(id, token as *mut c_void);
            }
        });
    }
    unsafe { cmd.decref(host) };
    RocList::empty()
}

/// Runtime thread only: turn the token into a Roc event and free the token.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_tick__complete(token: *mut c_void) -> Completion {
    // SAFETY: `token` came from `Box::into_raw` in this crate and is handed
    // back exactly once.
    let p = unsafe { Box::from_raw(token as *mut Pending) };
    TICKS.fetch_add(1, Ordering::Relaxed);
    Completion {
        request: p.request,
        route_key: RocStr::from_str(&p.route_key, abi::host()),
        event: TickEvent { payload: TickEventPayload { ticked: core::mem::ManuallyDrop::new(p.n) }, tag: TickEventTag::Ticked },
    }
}

/// The env block: read once per frame by the driver's env assembly.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_tick__env() -> TickEnv {
    TickEnv { ticks: TICKS.load(Ordering::Relaxed) }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_tick__gate(name: RocStr, argv: RocList<RocStr>) -> i32 {
    let host = abi::host();
    unsafe {
        name.decref(host);
        argv.decref(host);
    }
    -1
}
