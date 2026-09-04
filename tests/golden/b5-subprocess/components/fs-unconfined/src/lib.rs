//! roc:filesystem, UNCONFINED: the preopen is `/` -- ambient authority, basic-cli
//! parity. All op bodies live in fs-core; this crate only pins the root policy
//! and emits the no_mangle symbols (kept out of the shared rlib, H0c).
fs_core::exports!(fs_unconfined, fs_core::Root { base: std::path::PathBuf::from("/"), confined: false });
