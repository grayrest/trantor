//! Component `cli`: the driver. Owns the six runtime symbols, the process
//! main(), and the roc_main import. hematite assigns exactly one runtime
//! provider (H0c) and one main() to the driver. main() is generated: it would
//! run each component's init in topological order and teardown in reverse; this
//! two-host-component fixture has no stateful init, so main() is minimal, but
//! it wraps roc_main in catch_unwind and every boundary is extern "C-unwind"
//! (H0d).
#![allow(dead_code)]
use core::ffi::c_void;
use core::ptr;
use hematite_abi as abi;

#[unsafe(no_mangle)]
pub extern "C" fn roc_alloc(length: usize, alignment: usize) -> *mut c_void {
    abi::DefaultAllocators::roc_alloc(ptr::null_mut(), length, alignment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_dealloc(ptr_: *mut c_void, alignment: usize) {
    abi::DefaultAllocators::roc_dealloc(ptr::null_mut(), ptr_, alignment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_realloc(ptr_: *mut c_void, new_length: usize, alignment: usize) -> *mut c_void {
    abi::DefaultAllocators::roc_realloc(ptr::null_mut(), ptr_, new_length, alignment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_dbg(bytes: *const u8, len: usize) {
    let m = unsafe { core::slice::from_raw_parts(bytes, len) };
    eprintln!("[roc dbg] {}", String::from_utf8_lossy(m));
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_expect_failed(bytes: *const u8, len: usize) {
    let m = unsafe { core::slice::from_raw_parts(bytes, len) };
    eprintln!("[roc expect failed] {}", String::from_utf8_lossy(m));
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_crashed(bytes: *const u8, len: usize) {
    let m = unsafe { core::slice::from_raw_parts(bytes, len) };
    eprintln!("[ROC CRASHED] {}", String::from_utf8_lossy(m));
    std::process::exit(1);
}

// roc_main takes the args list; this fixture's driver passes an empty list and
// relies on the app's default. A real CLI driver would marshal argv into a
// RocList<RocStr>; kept minimal here since the golden app defaults its path.
unsafe extern "C-unwind" {
    fn roc_main() -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let outcome = std::panic::catch_unwind(|| unsafe { roc_main() });
    match outcome {
        Ok(code) => code,
        Err(_) => {
            eprintln!("[hematite] a component panicked; driver caught it at the boundary");
            70
        }
    }
}
