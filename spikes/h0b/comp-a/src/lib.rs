//! H0 component A + runtime + driver.
//!
//! This archive (liba.a) carries the six Roc runtime symbols, the process
//! `main` that calls into the app, and the hosted symbol `trantor__a__ping`.
//! Component B (libb.a) carries only `trantor__b__pong`. The spike's whole
//! question is whether `roc build` links BOTH archives into one binary with
//! hosted symbols resolved across the archive boundary (H0a + H0e).
#![allow(dead_code)]

mod abi;

use core::ffi::c_void;
use core::ptr;

// --- The six mandatory Roc runtime symbols (must live in exactly one archive) ---

#[unsafe(no_mangle)]
pub extern "C" fn roc_alloc(length: usize, alignment: usize) -> *mut c_void {
    abi::DefaultAllocators::roc_alloc(ptr::null_mut(), length, alignment)
}

/// # Safety: `ptr_` came from `roc_alloc` with the same `alignment`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_dealloc(ptr_: *mut c_void, alignment: usize) {
    abi::DefaultAllocators::roc_dealloc(ptr::null_mut(), ptr_, alignment)
}

/// # Safety: `ptr_` came from `roc_alloc` with the same `alignment`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_realloc(
    ptr_: *mut c_void,
    new_length: usize,
    alignment: usize,
) -> *mut c_void {
    abi::DefaultAllocators::roc_realloc(ptr::null_mut(), ptr_, new_length, alignment)
}

/// # Safety: `bytes`/`len` describe a readable slice for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_dbg(bytes: *const u8, len: usize) {
    let m = unsafe { core::slice::from_raw_parts(bytes, len) };
    eprintln!("[roc dbg] {}", String::from_utf8_lossy(m));
}

/// # Safety: `bytes`/`len` describe a readable slice for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_expect_failed(bytes: *const u8, len: usize) {
    let m = unsafe { core::slice::from_raw_parts(bytes, len) };
    eprintln!("[roc expect failed] {}", String::from_utf8_lossy(m));
}

/// # Safety: `bytes`/`len` describe a readable slice for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_crashed(bytes: *const u8, len: usize) {
    let m = unsafe { core::slice::from_raw_parts(bytes, len) };
    eprintln!("[ROC CRASHED] {}", String::from_utf8_lossy(m));
    std::process::exit(1);
}

// --- Hosted symbol for component A ---

/// `Host.ping! : {} => U64`. Distinctive value so the sum is unambiguous.
unsafe extern "C" { fn CFAbsoluteTimeGetCurrent() -> f64; }

#[unsafe(no_mangle)]
pub extern "C" fn trantor__a__ping() -> i64 {
    // Force a CoreFoundation dependency; value is deterministic (>0 -> 40).
    let t = unsafe { CFAbsoluteTimeGetCurrent() };
    if t > 0.0 { 40 } else { -1 }
}

// --- Provided entrypoint (defined by Roc) ---

unsafe extern "C" {
    fn roc_main() -> i64;
}

// --- Process entrypoint: the driver's generated main() ---

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let result = unsafe { roc_main() };
    println!("{result}");
    0
}
