//! Probe component A. Reproduces the H0c footgun: it exports an UNMANGLED,
//! plain-external global (`trantor_probe_collision`) that probe-b also defines,
//! so the two archives collide the way two vendored natives would. Its own
//! unique symbol keeps the archive distinct.
#[unsafe(no_mangle)]
pub extern "C" fn trantor_probe_collision() -> i64 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn trantor__probe_a__only() -> i64 {
    10
}
