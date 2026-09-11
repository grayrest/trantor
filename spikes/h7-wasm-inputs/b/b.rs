//! h7-wasm-inputs component `b`: the hosted leaf `Host.seed!`, in its OWN wasm
//! input. It allocates a RocStr through `roc_alloc` — defined in input `a` —
//! so the link has to resolve a reference from b into a as well as the app's
//! references into both.
#[path = "../a/roc_platform_abi.rs"]
#[allow(warnings)]
mod abi;

use core::ptr;

#[no_mangle]
pub extern "C" fn trantor__b__seed() -> i64 {
    let host = abi::make_roc_host(ptr::null_mut());
    // A heap string, built and released here: b -> a's roc_alloc/roc_dealloc.
    let s = abi::RocStr::from_str("twenty-one, allocated by component b", &host);
    let n = s.as_str().len() as i64; // 36
    unsafe { s.decref(&host) };
    n - 15 // 21, as gate-zero expects
}
