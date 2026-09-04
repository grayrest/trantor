//! H0 component B. This archive (libb.a) defines ONLY the hosted symbol
//! `hematite__b__pong` — no runtime symbols, no glue, no main. It proves a
//! second, independently built archive can contribute a hosted symbol that
//! the composed platform's `hosted {}` union names.

/// `Host.pong! : {} => U64`.
unsafe extern "C" {
    fn SecRandomCopyBytes(rnd: *const core::ffi::c_void, count: usize, bytes: *mut u8) -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn hematite__b__pong() -> i64 {
    // Force a Security dependency; kSecRandomDefault is NULL.
    let mut b = [0u8; 1];
    let _ = unsafe { SecRandomCopyBytes(core::ptr::null(), 1, b.as_mut_ptr()) };
    2
}
