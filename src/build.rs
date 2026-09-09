//! `hematite build <world-dir>` — the composed-platform build pipeline, driven
//! by the tool instead of each fixture's `build.sh`. It folds the H0c symbol
//! scan into its proper place: between `cargo` (which produces the archives) and
//! `roc build` (which links them), so a collision is rejected before the
//! memory-unsafe link, not after.
//!
//! Order (the canonical pipeline every fixture's build.sh open-coded):
//!   1. compose        — emit main.roc, the abi wrapper, the workspace, verbatim
//!                        interface + pure-Roc modules (in-process; D13).
//!   2. roc glue       — generate abi/src/generated.rs from the composed platform.
//!   3. cargo build    — build every host/driver component archive.
//!   4. stage          — copy the archives main.roc links into targets/ (exactly
//!                        the link set, so a stale archive from another world
//!                        can't leak in).
//!   5. framework sysroot — generate platform/targets/macos-sysroot from the
//!                        frameworks the world's components declare (roc links a
//!                        framework only from a bundled sysroot), or remove a
//!                        stale one when the world declares none.
//!   6. scan           — H0c archive symbol-collision scan.
//!   7. roc check      — typecheck the app before the linking build.
//!   8. roc build      — link the app against the composed platform.
//!
//! roc invocations carry the R5 timeout cap (`perl -e 'alarm 120; exec @ARGV'`),
//! the same gate the build.sh scripts used. ROC / GLUE come from the environment
//! (defaults `~/.bin/roc`, `~/.bin/RustGlue.roc`), matching those scripts.

use crate::manifest::World;
use crate::resolve::sanitize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Seconds a single `roc` invocation may run before the cap kills it (R5).
const ROC_TIMEOUT_SECS: &str = "120";
/// The `--target` that selects the wasm32 pipeline (roc's target name).
const WASM_TARGET: &str = "wasm32";

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn roc_bin() -> String {
    std::env::var("ROC").unwrap_or_else(|_| format!("{}/.bin/roc", home()))
}

fn glue_src() -> String {
    std::env::var("GLUE").unwrap_or_else(|_| format!("{}/.bin/RustGlue.roc", home()))
}

/// Run a command in `dir`, inheriting stdio, failing on a nonzero status.
fn run(program: &str, args: &[&str], dir: &Path, what: &str) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("{what}: spawn {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{what}: {program} exited {status}"));
    }
    Ok(())
}

/// Run `roc <args…>` under the R5 timeout cap, in `dir`.
fn roc_capped(args: &[&str], dir: &Path, what: &str) -> Result<(), String> {
    let roc = roc_bin();
    // perl -e 'alarm shift; exec @ARGV' 120 <roc> <args…>
    let mut full: Vec<&str> = vec![
        "-e",
        "alarm shift; exec @ARGV",
        ROC_TIMEOUT_SECS,
        roc.as_str(),
    ];
    full.extend_from_slice(args);
    run("perl", &full, dir, what)
}

/// The full pipeline. `dir` is the world directory; `app` its app subdir
/// (holding `main.roc`); `out` the binary name under `bin/`; `target` the
/// archive subdir under `platform/targets/`.
pub fn build(
    dir: &Path,
    world_file: &str,
    app: &str,
    out: &str,
    target: &str,
) -> Result<(), String> {
    // 1. compose (in-process).
    let world = crate::manifest::load_world(dir, world_file)?;
    let driver = crate::manifest::load_driver(dir, &world)?;
    let resolved = crate::resolve::resolve(dir, &world, &driver)?;
    crate::codegen::emit(dir, dir, &world, &driver, &resolved)?;
    eprintln!("hematite build: composed `{}`", world.world.name);

    // 2. roc glue -> abi/src/generated.rs.
    std::fs::create_dir_all(dir.join("glue-out"))
        .map_err(|e| format!("mkdir glue-out: {e}"))?;
    roc_capped(
        &["glue", &glue_src(), "glue-out", "platform/main.roc"],
        dir,
        "glue",
    )?;
    // Installed only when it changed, so an unchanged boundary does not
    // rebuild the abi crate and every host above it.
    let glue = std::fs::read(dir.join("glue-out/roc_platform_abi.rs")).map_err(|e| format!("read glue output: {e}"))?;
    crate::codegen::write_if_changed(&dir.join("abi/src/generated.rs"), &glue)?;

    // wasm32 (D-H7-9): its own cargo target, a merged host.wasm, and a wasm
    // link — see wasm.rs. No native staging, no framework sysroot.
    if target == WASM_TARGET {
        let work = crate::wasm::stage_host_wasm(dir, &world, &resolved)?;
        return crate::wasm::link_app(dir, world_file, &work, app, out, &roc_capped);
    }

    // 3. cargo build (no cap; roc alone carries R5) — in the world's own
    //    workspace or the host's (cargo_root), see cargo.rs.
    let built = crate::cargo::build(dir, &world, &resolved, None)?;

    // 4. stage exactly the archives main.roc links (resolved.archive_order),
    //    clearing stale ones so another world's archive can't leak in.
    stage_archives(dir, target, &built)?;

    // 5. macOS framework sysroot: generate it from the frameworks the world's
    //    components declare (e.g. turso's CoreFoundation), or remove a stale one
    //    so a prior world's sysroot can't leak into a framework-less link.
    sync_framework_sysroot(dir, &world)?;

    // 6. H0c symbol-collision scan — after the archives exist, before the link.
    crate::scan::scan(dir, world_file, None, target)?;

    // 7. roc check, then 8. roc build.
    let app_main = format!("{app}/main.roc");
    roc_capped(&["check", &app_main], dir, "roc check")?;
    std::fs::create_dir_all(dir.join("bin")).map_err(|e| format!("mkdir bin: {e}"))?;
    let out_flag = format!("--output=bin/{out}");
    roc_capped(&["build", &out_flag, &app_main], dir, "roc build")?;
    eprintln!("hematite build: linked bin/{out}");
    Ok(())
}

/// Generate `platform/targets/macos-sysroot` containing exactly the frameworks
/// the world's components declare, or remove a stale sysroot when none do.
///
/// roc's linker only links a framework from a platform-bundled sysroot, so a
/// component that links one (turso → CoreFoundation, via chrono/iana_time_zone)
/// declares it in `[components.<c>].frameworks`. The sysroot is built by
/// symlinking into the host SDK (`xcrun --show-sdk-path`): `usr` for libSystem,
/// and each framework's `.tbd` stub inside a REAL `.framework` directory (roc's
/// framework discovery skips symlinked directory entries). It is generated,
/// git-ignored, and rebuilt each time so it tracks the host SDK.
///
/// If no component declares a framework, any previously-generated sysroot is
/// removed so a prior world's build can't leak one into a framework-less link.
/// If the SDK can't be located (no Xcode / not macOS) the step is skipped; a
/// world that truly needs a framework then fails at the link, as before.
fn sync_framework_sysroot(dir: &Path, world: &World) -> Result<(), String> {
    use std::collections::BTreeSet;
    let frameworks: BTreeSet<&str> = world
        .components
        .values()
        .flat_map(|c| c.frameworks.iter())
        .map(String::as_str)
        .collect();
    let sysroot = dir.join("platform").join("targets").join("macos-sysroot");

    if frameworks.is_empty() {
        // Remove a stale sysroot from an earlier framework-linking world.
        if sysroot.exists() {
            let _ = std::fs::remove_dir_all(&sysroot);
        }
        return Ok(());
    }

    let sdk = match Command::new("xcrun").arg("--show-sdk-path").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    };
    if sdk.is_empty() {
        // No SDK to symlink from; leave it to the link to fail loudly if needed.
        eprintln!("hematite build: no macOS SDK found (xcrun); skipping framework sysroot");
        return Ok(());
    }
    let sdk = Path::new(&sdk);

    // Rebuild from scratch so a removed framework can't linger.
    let _ = std::fs::remove_dir_all(&sysroot);
    let frameworks_dir = sysroot.join("System/Library/Frameworks");
    std::fs::create_dir_all(&frameworks_dir)
        .map_err(|e| format!("mkdir {}: {e}", frameworks_dir.display()))?;
    // `usr` (libSystem) is reached by path, so a symlink is fine.
    symlink(&sdk.join("usr"), &sysroot.join("usr"))?;
    for fw in &frameworks {
        // The `.framework` must be a REAL dir (roc skips symlinked entries);
        // the `.tbd` stub inside it is symlinked from the SDK.
        let fw_dir = frameworks_dir.join(format!("{fw}.framework"));
        std::fs::create_dir_all(&fw_dir)
            .map_err(|e| format!("mkdir {}: {e}", fw_dir.display()))?;
        let tbd = format!("{fw}.tbd");
        symlink(
            &sdk.join(format!("System/Library/Frameworks/{fw}.framework/{tbd}")),
            &fw_dir.join(&tbd),
        )?;
    }
    Ok(())
}

/// Create a symlink at `link` pointing to `original`, replacing any existing one.
fn symlink(original: &Path, link: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(original, link)
        .map_err(|e| format!("symlink {} -> {}: {e}", link.display(), original.display()))
}

/// Copy the archives the composed `main.roc` links (the resolved component set,
/// each staged as `lib<sanitized component>.a` whatever cargo named it) into
/// `platform/targets/<target>/`, after clearing any previously-staged archives.
fn stage_archives(dir: &Path, target: &str, built: &[(String, PathBuf)]) -> Result<(), String> {
    let stage = dir.join("platform").join("targets").join(target);
    std::fs::create_dir_all(&stage).map_err(|e| format!("mkdir {}: {e}", stage.display()))?;
    // Clear stale archives (a prior world may have staged different ones).
    if let Ok(entries) = std::fs::read_dir(&stage) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("lib") && name.ends_with(".a") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    for (comp, from) in built {
        let archive = format!("lib{}.a", sanitize(comp));
        std::fs::copy(from, stage.join(&archive))
            .map_err(|e| format!("stage {archive} from {}: {e} (did cargo build it?)", from.display()))?;
    }
    Ok(())
}
