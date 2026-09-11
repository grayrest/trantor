//! Probe component B. Defines the SAME unmangled global as probe-a
//! (`trantor_probe_collision`) but with a different body — the memory-unsafe
//! case the scan must reject (the linker would silently pick one by archive
//! order). Its own unique symbol keeps the archive distinct.
#[unsafe(no_mangle)]
pub extern "C" fn trantor_probe_collision() -> i64 {
    2
}

#[unsafe(no_mangle)]
pub extern "C" fn trantor__probe_b__only() -> i64 {
    20
}
