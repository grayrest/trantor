//! roc:filesystem, CONFINED: the preopen is the process cwd and any path that
//! resolves outside it is PermissionDenied -- seahaven's confinement as a
//! swappable component rather than a fork (P14). Same op bodies (fs-core).
fs_core::exports!(fs_confined, fs_core::Root { base: std::env::current_dir().expect("cwd"), confined: true });
