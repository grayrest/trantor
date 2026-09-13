//! `svc-nudge` — one command and one event, neither with a payload
//! (D-H7-44): `Nudge.Poke`, answered with `NudgeEvent.Poked`.
use core::ffi::c_void;
use trantor_abi as abi;
use abi::services::{self, HostCtx};
use abi::{Nudge, NudgeEvent, RocList, RocStr};

type Answer = services::Answer<NudgeEvent>;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_nudge__init(_ctx: *const HostCtx) {}

#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)] // `Nudge` is zero-sized: Roc passes nothing
pub extern "C-unwind" fn trantor__svc_nudge__cmd(_request: u64, _cmd: Nudge) -> RocList<Answer> {
    let host = abi::host();
    let route_key = RocStr::from_str("nudge-key", host);
    unsafe { RocList::from_slice(&[Answer { route_key, event: NudgeEvent }], host) }
}

#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C-unwind" fn trantor__svc_nudge__complete(_token: *mut c_void) -> RocList<services::Completion<NudgeEvent>> {
    unreachable!("svc-nudge answers synchronously")
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_nudge__gate(name: RocStr, argv: RocList<RocStr>, _out: *mut RocStr) -> i32 {
    let host = abi::host();
    unsafe {
        name.decref(host);
        argv.decref(host);
    }
    -1
}
