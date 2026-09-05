//! SQ0 substrate host: the two roc:sqlite-unsound mechanisms in isolation.
//!   - `borrow_str!` returns a zero-copy borrowed slice over a static host
//!     buffer (rc==0 immortal); proves the compiler's incref/decref no-op on it.
//!   - `apply_i64!` invokes a boxed Roc closure through the erased-callable ABI;
//!     proves a HOST component can call back into Roc per-value (the fold reducer
//!     and turso scalar ride this).
use core::mem::MaybeUninit;
use hematite_abi as abi;
use abi::{RocErasedCallable, RocHost, RocStr};

/// A static host-owned buffer to borrow from — outlives every Roc reference.
static BORROWED: &[u8] = b"borrowed-hello";

/// `Probe.borrow_str! : {} => Str`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__probe_host__borrow_str() -> RocStr {
    // Zero-copy: the returned Str's bytes aim at BORROWED, its alloc-ptr at the
    // shared rc==0 static block. Roc reads it natively and never frees it.
    unsafe { abi::borrow::borrowed_str(BORROWED.as_ptr(), BORROWED.len()) }
}

/// The closure's arguments in Roc parameter order (one I64).
#[repr(C)]
struct ApplyArgs {
    arg0: i64,
}

/// `Probe.apply_i64! : I64, Box((I64 -> I64)) => I64`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__probe_host__apply_i64(arg0: i64, f: RocErasedCallable) -> i64 {
    let host = abi::host();
    let args = ApplyArgs { arg0 };
    let mut ret = MaybeUninit::<i64>::uninit();
    let r = unsafe {
        let payload = abi::roc_erased_callable_payload_ptr(f);
        let capture = abi::roc_erased_callable_capture_ptr(f);
        // Six-argument erased-callable shape: host, return slot, args, capture,
        // then the trailing reuse + descriptor out-params (both null = decline
        // reuse). Omitting the last two makes the callee decref an uninitialized
        // register.
        ((*payload).callable_fn_ptr)(
            host as *const RocHost as *mut RocHost,
            ret.as_mut_ptr() as *mut u8,
            &args as *const ApplyArgs as *const u8,
            capture,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        ret.assume_init()
    };
    // Owned hosted-fn argument (B0): release the closure exactly once.
    unsafe { abi::decref_erased_callable(f, host) };
    r
}

/// `Probe.print! : Str => {}`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__probe_host__print(s: RocStr) {
    println!("{}", s.as_str());
    unsafe { s.decref(abi::host()); } // owned arg; a borrowed str would decref-noop
}
