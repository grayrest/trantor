//! Component `marker`: a minimal host component for the HC0 features knob. Its
//! one hosted leaf prints, and a Cargo feature `extra` gates one extra symbol
//! whose presence in the archive proves the composed feature set took effect.
use hematite_abi as abi;
use abi::RocStr;

/// hosted `Mark.ping! : Str => {}`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__marker__ping(value: RocStr) {
    println!("ping: {}", value.as_str());
    unsafe { value.decref(abi::host()); } // owned arg (B0): released after use
}

/// Compiled only when the `extra` feature is on. The HC0 verify greps the
/// archive for this symbol to confirm the composer's features knob changed the
/// build.
#[cfg(feature = "extra")]
#[unsafe(no_mangle)]
pub extern "C" fn hematite__marker__extra_marker() {}
