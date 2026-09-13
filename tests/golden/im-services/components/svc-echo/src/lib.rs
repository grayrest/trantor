//! `svc-echo` — a SYNCHRONOUS service component (roc-solid's `notes` shape):
//! every `Echo.Ping` is answered with one `EchoEvent.Pong` from inside the
//! same call. Compiled once against its own glue type `Echo`; it never names
//! the driver (D-H7-7) and the only symbols it exports are its contract.
use core::ffi::c_void;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicPtr, Ordering};
use trantor_abi as abi;
use abi::services::{self, HostCtx};
use abi::{Echo, EchoEvent, EchoEventPayload, EchoEventTag, EchoTag, RocList, RocStr};

/// The contract types come from the generated shim (D-H7-7).
type Answer = services::Answer<EchoEvent>;

static CTX: AtomicPtr<HostCtx> = AtomicPtr::new(core::ptr::null_mut());

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_echo__init(ctx: *const HostCtx) {
    CTX.store(ctx as *mut HostCtx, Ordering::Release);
}

/// Consumes `cmd` (handed over, like a hosted call's arguments) and returns
/// owned answers the driver routes and releases. The route key is the
/// service's own field (`Ping(request_id, route_key, text)`), returned in the
/// answer (D-H7-17); the app's request id is ignored — `request` is host-minted.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_echo__cmd(_request: u64, cmd: Echo) -> RocList<Answer> {
    let host = abi::host();
    let (route_key, event) = match cmd.tag {
        EchoTag::Ping => {
            let p = unsafe { cmd.borrow_payload_ping_unchecked() };
            let reply = RocStr::from_str(p._2.as_str(), host);
            (RocStr::from_str(p._1.as_str(), host), EchoEvent { payload: EchoEventPayload { pong: ManuallyDrop::new(reply) }, tag: EchoEventTag::Pong })
        }
        EchoTag::Shout => {
            let s = unsafe { cmd.borrow_payload_shout_unchecked() };
            let loud = RocStr::from_str(&s.as_str().to_uppercase(), host);
            (RocStr::from_str("shout", host), EchoEvent {
                payload: EchoEventPayload { shouted: ManuallyDrop::new(abi::EchoEventShoutedPayload { _0: loud, _1: s.len() as u64 }) },
                tag: EchoEventTag::Shouted,
            })
        }
    };
    unsafe { cmd.decref(host) };
    let answers = [Answer { route_key, event }];
    unsafe { RocList::from_slice(&answers, host) }
}

/// A service with events must export `complete` even when it never wakes the
/// driver: the contract is uniform, so the shim can be generated without
/// knowing which services are asynchronous.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_echo__complete(_token: *mut c_void) -> RocList<services::Completion<EchoEvent>> {
    unreachable!("svc-echo answers synchronously and never wakes the driver")
}

/// The driver's data dir as this component sees it through `HostCtx.data_dir`
/// (D-H7-43), or empty when none was declared.
fn data_dir() -> String {
    let ctx = CTX.load(Ordering::Acquire);
    if ctx.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    // SAFETY: the driver's HostCtx is a static; `data_dir` returns bytes that
    // live for the rest of the process, or null.
    let ptr = unsafe { ((*ctx).data_dir)(&mut len) };
    if ptr.is_null() {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(ptr, len) }).into_owned()
}

/// Gate hook (D-H7-8): −1 = not mine. `data-dir-gate` answers with the data
/// dir the driver declared, so the fixture sees it cross the contract.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_echo__gate(name: RocStr, argv: RocList<RocStr>, out: *mut RocStr) -> i32 {
    let host = abi::host();
    let rc = match name.as_str() {
        "echo-gate" => 7,
        "data-dir-gate" => {
            // SAFETY: the chain hands a writable, empty RocStr (D-H7-40).
            unsafe { *out = RocStr::from_str(&data_dir(), host) };
            0
        }
        _ => -1,
    };
    unsafe {
        name.decref(host);
        argv.decref(host);
    }
    rc
}
