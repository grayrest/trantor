//! The one ABI crate (D4/D10): `roc glue` output plus a thin wrapper. Exactly
//! one of these exists per composition; every component host depends on it.
#![allow(dead_code, non_camel_case_types, improper_ctypes, improper_ctypes_definitions, unexpected_cfgs)]
mod generated;
pub use generated::*;

use std::sync::OnceLock;

// RocHost holds only fn pointers into this process; sharing one is sound.
unsafe impl Sync for RocHost {}
unsafe impl Send for RocHost {}

static HOST: OnceLock<RocHost> = OnceLock::new();

/// The process-wide RocHost (default allocators/handlers over the linker
/// runtime symbols). Cached; used to build owned Roc values (RocStr, ...).
pub fn host() -> &'static RocHost {
    HOST.get_or_init(|| make_roc_host(std::ptr::null_mut()))
}
