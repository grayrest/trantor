//! The hosted leaf `Host.seed!`, built inside the host workspace against
//! whichever world's abi hematite patched in.
use hematite_abi as abi;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__svc__seed() -> i32 {
    let host = abi::host();
    let s = abi::RocStr::from_str("twenty-one, from the host workspace", host);
    let n = s.as_str().len() as i32; // 35
    unsafe { s.decref(host) };
    n - 14
}
