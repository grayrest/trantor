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
//!   5. prelink hook   — if <world-dir>/prelink.sh exists, run it (a fixture's
//!                        native-link setup, e.g. the turso macOS sysroot).
//!   6. scan           — H0c archive symbol-collision scan.
//!   7. roc check      — typecheck the app before the linking build.
//!   8. roc build      — link the app against the composed platform.
//!
//! roc invocations carry the R5 timeout cap (`perl -e 'alarm 120; exec @ARGV'`),
//! the same gate the build.sh scripts used. ROC / GLUE come from the environment
//! (defaults `~/.bin/roc`, `~/.bin/RustGlue.roc`), matching those scripts.

use crate::resolve::{sanitize, Resolved};
use std::path::Path;
use std::process::Command;

/// Seconds a single `roc` invocation may run before the cap kills it (R5).
const ROC_TIMEOUT_SECS: &str = "120";

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
    let driver = crate::manifest::load_driver(dir, &world.world.driver)?;
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
    std::fs::copy(
        dir.join("glue-out/roc_platform_abi.rs"),
        dir.join("abi/src/generated.rs"),
    )
    .map_err(|e| format!("copy generated.rs: {e}"))?;

    // 3. cargo build (no cap; roc alone carries R5).
    run("cargo", &["build", "--release"], dir, "cargo build")?;

    // 4. stage exactly the archives main.roc links (resolved.archive_order),
    //    clearing stale ones so another world's archive can't leak in.
    stage_archives(dir, target, &resolved)?;

    // 5. optional fixture-specific native-link setup (e.g. the turso sysroot).
    let prelink = dir.join("prelink.sh");
    if prelink.exists() {
        run("bash", &["prelink.sh"], dir, "prelink.sh")?;
    }

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

/// Copy the archives the composed `main.roc` links (the resolved component set,
/// each `lib<sanitized>.a`) from `target/release/` into
/// `platform/targets/<target>/`, after clearing any previously-staged archives.
fn stage_archives(dir: &Path, target: &str, r: &Resolved) -> Result<(), String> {
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
    let built = dir.join("target").join("release");
    for comp in &r.archive_order {
        let archive = format!("lib{}.a", sanitize(comp));
        let from = built.join(&archive);
        std::fs::copy(&from, stage.join(&archive))
            .map_err(|e| format!("stage {archive}: {e} (did cargo build it?)"))?;
    }
    Ok(())
}
