//! Component `env`: a host component exposing the process's first CLI argument
//! as a single Str (dodging this glue version's lack of a host-built
//! RocList<RocStr> — see notes/2026-09-04-h2-reference-platform.md). A richer
//! composition would return the whole List(OsStr); the upstream glue redesign
//! adds the list-of-refcounted constructor that makes that a one-liner.
use trantor_abi as abi;
use abi::RocStr;

/// hosted `Env.path_arg! : {} => Str`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__env__path_arg() -> RocStr {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "README.md".to_string());
    RocStr::from_str(&arg, abi::host())
}
