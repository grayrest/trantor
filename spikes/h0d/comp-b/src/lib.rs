//! H0d component B: a hosted call that panics, guarding a local RAII resource.
//! Two build configs select the ABI:
//!   default            -> extern "C"        (panic across it = abort)
//!   --cfg cunwind      -> extern "C-unwind" (panic unwinds across the boundary)

struct GuardB;
impl Drop for GuardB {
    fn drop(&mut self) {
        eprintln!("B::drop ran (component B local teardown)");
    }
}

#[cfg(not(cunwind))]
#[unsafe(no_mangle)]
pub extern "C" fn hematite__b__boom() -> i64 {
    let _g = GuardB;
    panic!("boom from component B");
}

#[cfg(cunwind)]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__b__boom() -> i64 {
    let _g = GuardB;
    panic!("boom from component B");
}
