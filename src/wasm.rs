//! `hematite build --target wasm32` — the wasm32 half of the pipeline
//! (D-H7-9 revised, measured in `spikes/h7-wasm-inputs`). roc links its wasm
//! inputs `--whole-archive`, so per-component inputs collide on std and the
//! compiler builtins; hematite therefore merges every component into ONE
//! `platform/targets/wasm32/host.wasm`:
//!
//!   1. `cargo build --release --target wasm32-unknown-unknown` (panic=abort);
//!   2. per component, extract the staticlib with `llvm-ar`, keep only real
//!      wasm members (rustc bundles host-arch ELF compiler_builtins objects
//!      that wasm-ld chokes on) and re-archive them;
//!   3. find each non-driver component's ROOT members — those defining a
//!      symbol it owns (`hematite__<c>__*`: its hosted leaves and service
//!      contract), via `llvm-nm`;
//!   4. `wasm-ld -r --whole-archive <driver>.a --no-whole-archive <roots…>
//!      <component>.a…` — the driver whole, each component pulled in from its
//!      roots, everything else lazy so std stays single-copy;
//!   5. the H0c scan over the wasm archives, `roc check`, `roc build
//!      --target=wasm32` → `bin/<out>.wasm`.
//!
//! Tools: `LLVM_BIN` (default Homebrew llvm) for `llvm-ar`/`llvm-nm`;
//! `wasm-ld` from `LLVM_BIN` when present, else `PATH`.

use crate::resolve::{sanitize, Resolved};
use crate::symbols::{llvm_tool, Format};
use std::path::{Path, PathBuf};
use std::process::Command;

const WASM_TRIPLE: &str = "wasm32-unknown-unknown";
/// `\0asm`: the magic that separates a wasm member from a host-arch ELF blob.
const WASM_MAGIC: &[u8] = b"\0asm";
/// Scratch dir (under the world's cargo target dir) for extracted members.
const WORK_DIR: &str = "target/hematite-wasm";

fn wasm_ld() -> PathBuf {
    let in_llvm = llvm_tool("wasm-ld");
    if in_llvm.exists() {
        in_llvm
    } else {
        PathBuf::from("wasm-ld")
    }
}

fn run(program: &Path, args: &[&str], dir: &Path, what: &str) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("{what}: spawn {}: {e}", program.display()))?;
    if !status.success() {
        return Err(format!("{what}: {} exited {status}", program.display()));
    }
    Ok(())
}

/// Steps 1–4: the merged `platform/targets/wasm32/host.wasm`. Returns the
/// directory holding the per-component wasm archives (for the scan).
pub fn stage_host_wasm(dir: &Path, r: &Resolved) -> Result<PathBuf, String> {
    // Every path below is handed to a tool running in `dir`, so make them
    // absolute once rather than relative-to-relative.
    let dir = &dir.canonicalize().map_err(|e| format!("canonicalize {}: {e}", dir.display()))?;
    // 1. cargo, every member, for wasm32. panic=abort: no unwinding on wasm.
    let status = Command::new("cargo")
        .args(["build", "--release", "--target", WASM_TRIPLE])
        .env("CARGO_PROFILE_RELEASE_PANIC", "abort")
        .current_dir(dir)
        .status()
        .map_err(|e| format!("cargo build (wasm32): spawn: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build (wasm32): cargo exited {status}"));
    }

    // 2. wasm-only archives per component.
    let work = dir.join(WORK_DIR);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("mkdir {}: {e}", work.display()))?;
    let built = dir.join("target").join(WASM_TRIPLE).join("release");
    let mut archives: Vec<(String, PathBuf, Vec<PathBuf>)> = Vec::new();
    for comp in &r.archive_order {
        let lib = format!("lib{}.a", sanitize(comp));
        let members_dir = work.join(sanitize(comp));
        std::fs::create_dir_all(&members_dir).map_err(|e| format!("mkdir {}: {e}", members_dir.display()))?;
        let out_flag = format!("--output={}", members_dir.display());
        run(
            &llvm_tool("llvm-ar"),
            &["x", &out_flag, built.join(&lib).to_str().unwrap_or_default()],
            dir,
            &format!("extract {lib}"),
        )?;
        let members = wasm_members(&members_dir)?;
        if members.is_empty() {
            return Err(format!("{lib}: no wasm members (did cargo build it for {WASM_TRIPLE}?)"));
        }
        let archive = work.join(&lib);
        let mut args: Vec<String> = vec!["rcs".into(), archive.display().to_string()];
        args.extend(members.iter().map(|m| m.display().to_string()));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run(&llvm_tool("llvm-ar"), &refs, dir, &format!("archive {lib}"))?;
        archives.push((comp.clone(), archive, members));
    }

    // 3 + 4. The merge: driver whole (first in archive_order), components rooted.
    let out = dir.join("platform").join("targets").join("wasm32");
    std::fs::create_dir_all(&out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
    let mut args: Vec<String> = vec!["-r".into(), "--whole-archive".into()];
    let (driver, driver_archive, _) = &archives[0];
    debug_assert_eq!(driver, &r.driver);
    args.push(driver_archive.display().to_string());
    args.push("--no-whole-archive".into());
    for (comp, archive, members) in archives.iter().skip(1) {
        let prefix = format!("hematite__{}__", sanitize(comp));
        let roots = root_members(members, &prefix)?;
        if roots.is_empty() {
            return Err(format!(
                "component `{comp}` defines no `{prefix}*` symbol in any wasm member — nothing roots \
                 it into host.wasm"
            ));
        }
        args.extend(roots.iter().map(|m| m.display().to_string()));
        args.push(archive.display().to_string());
    }
    args.push("-o".into());
    args.push(out.join("host.wasm").display().to_string());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&wasm_ld(), &refs, dir, "wasm-ld merge")?;
    eprintln!("hematite build: merged {} archives into platform/targets/wasm32/host.wasm", archives.len());
    Ok(work)
}

/// The real wasm objects in an extracted staticlib, sorted for determinism.
fn wasm_members(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let mut magic = [0u8; 4];
        let ok = std::fs::File::open(&p)
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut magic))
            .is_ok();
        if ok && magic == WASM_MAGIC {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

/// Members defining a symbol with the component's `hematite__<c>__` prefix.
fn root_members(members: &[PathBuf], prefix: &str) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    for m in members {
        let out = Command::new(llvm_tool("llvm-nm"))
            .args(["--defined-only", m.to_str().unwrap_or_default()])
            .output()
            .map_err(|e| format!("llvm-nm {}: {e}", m.display()))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let defines_root = text.lines().any(|l| {
            let mut it = l.split_whitespace();
            matches!((it.next(), it.next(), it.next()), (Some(_), Some("T"), Some(n)) if n.starts_with(prefix))
        });
        if defines_root {
            roots.push(m.clone());
        }
    }
    Ok(roots)
}

/// Step 5: scan the wasm archives, then check and link the app.
pub fn link_app(
    dir: &Path,
    world_file: &str,
    work: &Path,
    app: &str,
    out: &str,
    roc_capped: &dyn Fn(&[&str], &Path, &str) -> Result<(), String>,
) -> Result<(), String> {
    crate::scan::scan_archives(dir, world_file, work, Format::Wasm)?;
    let app_main = format!("{app}/main.roc");
    roc_capped(&["check", &app_main], dir, "roc check")?;
    std::fs::create_dir_all(dir.join("bin")).map_err(|e| format!("mkdir bin: {e}"))?;
    let out_flag = format!("--output=bin/{out}.wasm");
    roc_capped(&["build", "--target=wasm32", &out_flag, &app_main], dir, "roc build (wasm32)")?;
    eprintln!("hematite build: linked bin/{out}.wasm");
    Ok(())
}
