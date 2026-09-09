//! Driver `a`: roc-solid's wasm gate-zero host minus the hosted leaf (that is
//! component `b`, a second archive the merge folds in). Provides the runtime
//! symbols and the wasm-bindgen-free surface `run.mjs` calls; on the host arch
//! it also builds as an rlib so the workspace lints it.
use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use hematite_abi as abi;

type App = abi::AnonStruct5f5555ad42ba4a42;

/// Counts `roc_alloc` calls so the runner can confirm the app's string build
/// really went through the host allocator on wasm32.
static ROC_ALLOCS: AtomicUsize = AtomicUsize::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn roc_alloc(length: usize, alignment: usize) -> *mut c_void {
    ROC_ALLOCS.fetch_add(1, Ordering::Relaxed);
    abi::DefaultAllocators::roc_alloc(ptr::null_mut(), length, alignment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_dealloc(p: *mut c_void, alignment: usize) {
    abi::DefaultAllocators::roc_dealloc(ptr::null_mut(), p, alignment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_realloc(p: *mut c_void, new_length: usize, alignment: usize) -> *mut c_void {
    abi::DefaultAllocators::roc_realloc(ptr::null_mut(), p, new_length, alignment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_dbg(bytes: *const u8, len: usize) {
    abi::DefaultHandlers::roc_dbg(ptr::null_mut(), bytes, len)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_expect_failed(bytes: *const u8, len: usize) {
    abi::DefaultHandlers::roc_expect_failed(ptr::null_mut(), bytes, len)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roc_crashed(bytes: *const u8, len: usize) {
    abi::DefaultHandlers::roc_crashed(ptr::null_mut(), bytes, len)
}

/// The last app record, kept so `wasm_main`'s pointer stays valid.
static mut LAST: Option<App> = None;

/// Call the Roc entrypoint; return a pointer to the message bytes.
#[unsafe(no_mangle)]
pub extern "C" fn wasm_main() -> *const u8 {
    let app = unsafe { abi::roc_app() };
    let p = app.message.as_slice().as_ptr();
    unsafe { LAST = Some(app) };
    p
}

#[unsafe(no_mangle)]
pub extern "C" fn wasm_result_len() -> usize {
    unsafe { (*ptr::addr_of!(LAST)).as_ref().map_or(0, |a| a.message.as_slice().len()) }
}

#[unsafe(no_mangle)]
pub extern "C" fn wasm_n() -> i64 {
    unsafe { (*ptr::addr_of!(LAST)).as_ref().map_or(0, |a| a.n) }
}

#[unsafe(no_mangle)]
pub extern "C" fn wasm_alloc_count() -> usize {
    ROC_ALLOCS.load(Ordering::Relaxed)
}

/// Release the retained record (exercises the decref / `roc_dealloc` path).
#[unsafe(no_mangle)]
pub extern "C" fn wasm_release() {
    unsafe {
        if let Some(app) = (*ptr::addr_of_mut!(LAST)).take() {
            app.decref(abi::host());
        }
    }
}
