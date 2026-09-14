//! `svc-chime` — a service whose command and event unions each have ONE
//! variant with one record field (D-H7-45): `Chime.Strike({ key, n })`,
//! answered with `ChimeEvent.Struck({ seq, last, ok, body })`. Glue names both
//! types after their records.
use core::ffi::c_void;
use trantor_abi as abi;
use abi::services::{self, HostCtx};
use abi::{Chime, ChimeEvent, RocList, RocStr};

type Answer = services::Answer<ChimeEvent>;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_chime__init(_ctx: *const HostCtx) {}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_chime__cmd(_request: u64, cmd: Chime) -> RocList<Answer> {
    let host = abi::host();
    let route_key = RocStr::from_str(cmd.key.as_str(), host);
    let body = RocStr::from_str("struck, in a string long enough to live on the heap", host);
    let event: ChimeEvent = abi::ChimeEventStruck { seq: cmd.n, last: true, ok: true, body };
    unsafe { cmd.decref(host) };
    unsafe { RocList::from_slice(&[Answer { route_key, event }], host) }
}

#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C-unwind" fn trantor__svc_chime__complete(_token: *mut c_void) -> RocList<services::Completion<ChimeEvent>> {
    unreachable!("svc-chime answers synchronously")
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_chime__gate(name: RocStr, argv: RocList<RocStr>, _out: *mut RocStr) -> i32 {
    let host = abi::host();
    unsafe {
        name.decref(host);
        argv.decref(host);
    }
    -1
}
