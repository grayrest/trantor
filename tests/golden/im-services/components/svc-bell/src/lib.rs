//! `svc-bell` — a service whose command and event unions each have ONE
//! variant (D-H7-44): `Bell.Ring(request_id, route_key, text)`, answered with
//! `BellEvent.Rang(text)`. Its types are the payloads themselves: `Bell` is the
//! three fields, `BellEvent` is the `RocStr`.
use core::ffi::c_void;
use trantor_abi as abi;
use abi::services::{self, HostCtx};
use abi::{Bell, BellEvent, RocList, RocStr};

type Answer = services::Answer<BellEvent>;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_bell__init(_ctx: *const HostCtx) {}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_bell__cmd(_request: u64, cmd: Bell) -> RocList<Answer> {
    let host = abi::host();
    let route_key = RocStr::from_str(cmd._1.as_str(), host);
    let event: BellEvent = RocStr::from_str(&format!("{}:{}", cmd._2.as_str(), cmd._0), host);
    unsafe { cmd.decref(host) };
    unsafe { RocList::from_slice(&[Answer { route_key, event }], host) }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_bell__complete(_token: *mut c_void) -> RocList<services::Completion<BellEvent>> {
    unreachable!("svc-bell answers synchronously")
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__svc_bell__gate(name: RocStr, argv: RocList<RocStr>, _out: *mut RocStr) -> i32 {
    let host = abi::host();
    unsafe {
        name.decref(host);
        argv.decref(host);
    }
    -1
}
