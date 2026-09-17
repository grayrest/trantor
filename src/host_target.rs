//! The roc target this machine builds for, and the native targets every
//! composed platform declares.
//!
//! cargo builds the component archives for the HOST triple (trantor never
//! passes it `--target` on the native path), so the only native target whose
//! staged archives are real is the host's. The default `--target` is therefore
//! the host's roc target name, not one developer's `arm64mac`: on a Linux host
//! that default staged Linux archives under the Mac name and linked nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Native targets every composed `main.roc` declares, whatever machine
/// composed it, so a composed platform is not specific to its host. roc accepts
/// an entry whose archives were never staged; it only reads the inputs of the
/// target it links.
///
/// glibc, not musl, for Linux: a Rust staticlib built on a glibc host links
/// against the system libc dynamically, which is what roc's glibc targets do.
/// A musl host adds its own target through [`host_target`].
pub const DECLARED_NATIVE_TARGETS: &[&str] = &["arm64mac", "x64mac", "arm64glibc", "x64glibc"];

/// The roc target name for the machine running trantor, from the OS and
/// architecture it runs on. Linux's libc cannot be read from the OS name, so it
/// is the C environment trantor itself was built for: cargo builds the
/// components with the same host toolchain, so the two agree.
pub fn host_target() -> Result<&'static str, String> {
    let is_musl = cfg!(target_env = "musl");
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("arm64mac"),
        ("macos", "x86_64") => Ok("x64mac"),
        ("linux", "aarch64") => Ok(if is_musl { "arm64musl" } else { "arm64glibc" }),
        ("linux", "x86_64") => Ok(if is_musl { "x64musl" } else { "x64glibc" }),
        (os, arch) => Err(format!(
            "no roc target is known for this host ({os}/{arch}); pass `--target <roc target>`"
        )),
    }
}

/// C runtime files a glibc target's `inputs` name before the archives: the
/// PIE entry point and the `.init` prologue.
pub const GLIBC_START_FILES: &[&str] = &["Scrt1.o", "crti.o"];

/// C runtime files a glibc target's `inputs` name after the app: the `.fini`
/// epilogue, then the libraries a Rust staticlib needs from the system. The
/// set is rustc's `native-static-libs` for linux-gnu (`-lgcc_s -lutil -lrt
/// -lpthread -lm -ldl -lc`) as glibc 2.34+ ships it, where util, rt, pthread
/// and dl live inside libc. `libc.so` is glibc's linker script, which brings
/// `libc_nonshared.a` and the dynamic loader along; `libgcc_s.so.1` is the
/// library itself because its `libgcc_s.so` script says `-lgcc`, which roc's
/// link has no search path to resolve.
pub const GLIBC_END_FILES: &[&str] = &["crtn.o", "libc.so", "libm.so.6", "libgcc_s.so.1"];

/// A glibc target needs the C runtime staged beside its archives.
pub fn is_glibc(target: &str) -> bool {
    target.ends_with("glibc")
}

/// Whether a staged file is one of the glibc runtime files trantor put there.
pub fn is_glibc_runtime_file(file: &str) -> bool {
    GLIBC_START_FILES.iter().chain(GLIBC_END_FILES).any(|f| *f == file)
}

/// Copy the glibc runtime files into a target's staging dir, from wherever the
/// host C compiler finds them (`cc -print-file-name`, so the multiarch layout
/// of each distribution is the compiler's business). Copied fresh on every
/// build, so a libc upgrade reaches the next link.
pub fn stage_glibc_runtime(stage: &Path) -> Result<(), String> {
    for file in GLIBC_START_FILES.iter().chain(GLIBC_END_FILES) {
        let from = host_c_runtime_file(file)?;
        std::fs::copy(&from, stage.join(file))
            .map_err(|e| format!("stage {file} from {}: {e}", from.display()))?;
    }
    Ok(())
}

/// The host path of one C runtime file. `cc -print-file-name` echoes the bare
/// name back when it finds nothing, which is reported as the file missing
/// rather than handed to a copy that fails on a relative path.
fn host_c_runtime_file(file: &str) -> Result<PathBuf, String> {
    let out = Command::new("cc")
        .arg(format!("-print-file-name={file}"))
        .output()
        .map_err(|e| format!("find {file}: spawn cc: {e} (a glibc link needs a C toolchain)"))?;
    let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    if !out.status.success() || !path.is_absolute() || !path.is_file() {
        return Err(format!(
            "no {file} on this host: `cc -print-file-name={file}` did not find it. A glibc target links \
             the host's C runtime; install the libc development files (e.g. `apt install build-essential`)."
        ));
    }
    Ok(path)
}

/// `--target` when one was given, else the host's.
pub fn target_or_host(given: Option<String>) -> Result<String, String> {
    match given {
        Some(t) => Ok(t),
        None => host_target().map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_target_is_declared_by_every_platform_on_a_supported_host() {
        let host = host_target().expect("tests run on a supported host");
        let is_musl_host = host.ends_with("musl");
        assert!(is_musl_host || DECLARED_NATIVE_TARGETS.contains(&host));
    }

    #[test]
    fn an_explicit_target_wins_over_the_host() {
        assert_eq!(target_or_host(Some("wasm32".into())).unwrap(), "wasm32");
    }
}
