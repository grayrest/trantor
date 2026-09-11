//! Component `b`: the hosted leaf `Host.seed!` in its own archive. It
//! allocates a RocStr through `roc_alloc` — defined by the driver — so the
//! wasm merge has to resolve a reference from b into a as well as the app's
//! references into both.
use trantor_abi as abi;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__b__seed() -> i64 {
    let host = abi::host();
    let s = abi::RocStr::from_str("twenty-one, allocated by component b", host);
    let n = s.as_str().len() as i64; // 36
    unsafe { s.decref(host) };
    n - 15 // 21, as gate-zero expects
}
