//! Counts live host resources so the acceptance test can prove they all drop.
use trantor_abi as abi;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__gauge_host__live() -> i32 {
    abi::resource::live() as i32
}
