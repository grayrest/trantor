//! `svc-echo` — a SYNCHRONOUS service component (roc-solid's `notes` shape):
//! every `Echo.Ping` is answered with one `EchoEvent.Pong` from inside the
//! same call. Compiled once against its own glue type `Echo`; it never names
//! the driver (D-H7-7) and the only symbols it exports are its contract.
use core::ffi::c_void;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicPtr, Ordering};
use hematite_abi as abi;
use abi::{Echo, EchoEvent, EchoEventPayload, EchoEventTag, EchoTag, RocList, RocStr};

#[repr(C)]
pub struct HostCtx {
    pub component_id: u32,
    pub wake: extern "C" fn(u32, *mut c_void),
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Answer { pub route_key: RocStr, pub event: EchoEvent }

static CTX: AtomicPtr<HostCtx> = AtomicPtr::new(core::ptr::null_mut());

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_echo__init(ctx: *const HostCtx) {
    CTX.store(ctx as *mut HostCtx, Ordering::Release);
}

/// Consumes `cmd` and `route_key` (they are handed over, like a hosted call's
/// arguments) and returns owned answers the driver routes and releases.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_echo__cmd(_request: u64, route_key: RocStr, cmd: Echo) -> RocList<Answer> {
    let host = abi::host();
    let event = match cmd.tag {
        EchoTag::Ping => {
            let p = unsafe { cmd.borrow_payload_ping_unchecked() };
            let reply = RocStr::from_str(p._2.as_str(), host);
            EchoEvent { payload: EchoEventPayload { pong: ManuallyDrop::new(reply) }, tag: EchoEventTag::Pong }
        }
        EchoTag::Shout => {
            let s = unsafe { cmd.borrow_payload_shout_unchecked() };
            let loud = RocStr::from_str(&s.as_str().to_uppercase(), host);
            EchoEvent {
                payload: EchoEventPayload { shouted: ManuallyDrop::new(abi::EchoEventShoutedPayload { _0: loud, _1: s.len() as u64 }) },
                tag: EchoEventTag::Shouted,
            }
        }
    };
    unsafe { cmd.decref(host) };
    let answers = [Answer { route_key, event }];
    unsafe { RocList::from_slice(&answers, host) }
}

/// Gate hook (D-H7-8): −1 = not mine.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc_echo__gate(name: RocStr, argv: RocList<RocStr>) -> i32 {
    let host = abi::host();
    let rc = if name.as_str() == "echo-gate" { 7 } else { -1 };
    unsafe {
        name.decref(host);
        argv.decref(host);
    }
    rc
}
