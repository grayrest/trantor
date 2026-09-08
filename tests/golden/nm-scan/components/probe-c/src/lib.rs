//! Probe component C. Shares NO unmangled global with probe-a — only its own
//! uniquely-named symbol — so a world of {probe-a, probe-c} scans clean. The
//! positive control that the scan does not flag ordinary distinct components.
#[unsafe(no_mangle)]
pub extern "C" fn hematite__probe_c__only() -> i64 {
    30
}
