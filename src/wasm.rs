//! `trantor build --target wasm32` — the wasm32 half of the pipeline
//! (D-H7-9 revised, measured in `spikes/h7-wasm-inputs`). roc links its wasm
//! inputs `--whole-archive`, so per-component inputs collide on std and the
//! compiler builtins; trantor therefore merges every component into ONE
//! `platform/targets/wasm32/host.wasm`:
//!
//!   1. `cargo build --release --target wasm32-unknown-unknown` (panic=abort);
//!   2. per component, extract the staticlib with `llvm-ar`, keep only real
//!      wasm members (rustc bundles host-arch ELF compiler_builtins objects
//!      that wasm-ld chokes on) and re-archive them;
//!   3. find each non-driver component's ROOT members — those defining a
//!      symbol it owns (`trantor__<c>__*`: its hosted leaves and service
//!      contract), via `llvm-nm`;
//!   4. `wasm-ld -r --whole-archive <driver>.a --no-whole-archive <roots…>
//!      <component>.a…` — the driver whole, each component pulled in from its
//!      roots, everything else lazy so std stays single-copy;
//!   5. the H0c scan over the wasm archives, `roc check`, `roc build
//!      --target=wasm32` → `bin/<out>.wasm`.
//!
//! Tools: `LLVM_BIN` (default Homebrew llvm, else `PATH`) for `llvm-ar`/`llvm-nm`;
//! `wasm-ld` from `LLVM_BIN` when present, else `PATH`.

use crate::manifest::World;
use crate::resolve::{sanitize, Resolved};
use crate::symbols::{llvm_tool, Format};
use std::path::{Path, PathBuf};
use std::process::Command;

const WASM_TRIPLE: &str = "wasm32-unknown-unknown";
/// `\0asm`: the magic that separates a wasm member from a host-arch ELF blob.
const WASM_MAGIC: &[u8] = b"\0asm";
/// Scratch dir (under the world's cargo target dir) for extracted members.
const WORK_DIR: &str = "target/trantor-wasm";

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
pub fn stage_host_wasm(dir: &Path, gen: &Path, world: &World, r: &Resolved) -> Result<PathBuf, String> {
    // Every path below is handed to a tool running in `dir`, so make them
    // absolute once rather than relative-to-relative. `dir` is the SOURCE
    // tree; `gen` is `target/trantor/<world>`, where everything is written.
    let dir = &dir.canonicalize().map_err(|e| format!("canonicalize {}: {e}", dir.display()))?;
    std::fs::create_dir_all(gen).map_err(|e| format!("mkdir {}: {e}", gen.display()))?;
    let gen = &gen.canonicalize().map_err(|e| format!("canonicalize {}: {e}", gen.display()))?;
    // 1. cargo, every component, for wasm32 (panic=abort: no unwinder there) —
    //    under the workspace build lock until the members are extracted
    //    (D-H7-34).
    let lock = crate::cargo::build_lock(dir, world)?;
    let built = crate::cargo::build(dir, gen, world, r, Some(WASM_TRIPLE))?;

    // 2. wasm-only archives per component.
    let work = gen.join(WORK_DIR);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("mkdir {}: {e}", work.display()))?;
    let mut archives: Vec<(String, PathBuf, Vec<PathBuf>)> = Vec::new();
    for (comp, from) in &built {
        let lib = format!("lib{}.a", sanitize(comp));
        let members_dir = work.join(sanitize(comp));
        std::fs::create_dir_all(&members_dir).map_err(|e| format!("mkdir {}: {e}", members_dir.display()))?;
        let out_flag = format!("--output={}", members_dir.display());
        run(
            &llvm_tool("llvm-ar"),
            &["x", &out_flag, from.to_str().unwrap_or_default()],
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
    drop(lock);

    // One heap for the merged module, before anything links against it.
    check_one_allocator(&archives)?;

    // 3 + 4. The merge: driver whole (first in archive_order), components rooted.
    let out = gen.join("platform").join("targets").join("wasm32");
    std::fs::create_dir_all(&out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
    // `--strip-debug`: the relocatable carries every member's DWARF otherwise,
    // and nothing downstream reads it — roc's final link is what `wasm-opt`
    // runs on. Measured on roc-solid's DOM host (D25): the difference between
    // a 500 KB and a 40 KB host object.
    // `--allow-multiple-definition`: first wins, which is what the roc link
    // does with archives anyway. Needed because a size-correct build (LTO
    // into one codegen unit per component) leaves each component's object
    // carrying std's externally-visible runtime singletons —
    // `rust_eh_personality`, `std::panicking::EMPTY_PANIC` — which the driver
    // already defines (D-H7-13: one runtime, the driver's). The H0c scan is
    // still the collision policy for everything that is not a singleton.
    let mut args: Vec<String> = vec![
        "-r".into(),
        "--whole-archive".into(),
        "--strip-debug".into(),
        "--allow-multiple-definition".into(),
    ];
    let (driver, driver_archive, _) = &archives[0];
    debug_assert_eq!(driver, &r.driver);
    args.push(driver_archive.display().to_string());
    args.push("--no-whole-archive".into());
    for (comp, archive, members) in archives.iter().skip(1) {
        let prefix = format!("trantor__{}__", sanitize(comp));
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
    eprintln!("trantor build: merged {} archives into platform/targets/wasm32/host.wasm", archives.len());
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

/// Members defining a symbol with the component's `trantor__<c>__` prefix.
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

/// Refuse a merge in which a component carries its OWN copy of std's allocator
/// state (D-H7-41).
///
/// `std::sys::alloc::wasm::DLMALLOC` is the heap. Merged components share one
/// linear memory, so there must be exactly one — which `--allow-multiple-
/// definition` gives, first-wins, as long as the symbol stays GLOBAL. Fat LTO
/// internalizes it to a local symbol, which the linker cannot unify: each
/// component then hands out the same memory twice, and the module traps with
/// `memory access out of bounds` on the first allocation after a second
/// component has allocated. That cost a day to find, so it is checked rather
/// than remembered.
fn check_one_allocator(archives: &[(String, PathBuf, Vec<PathBuf>)]) -> Result<(), String> {
    const ALLOC_STATE: &str = "3sys5alloc4wasm8DLMALLOC";
    let nm = llvm_tool("llvm-nm");
    let mut private = Vec::new();
    for (comp, archive, _) in archives {
        let out = Command::new(&nm)
            .arg("--defined-only")
            .arg(archive)
            .output()
            .map_err(|e| format!("run {} on {}: {e}", nm.display(), archive.display()))?;
        let mut saw = false;
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() == 3 && f[2].contains(ALLOC_STATE) {
                saw = true;
                // Lowercase is a local binding: private to this object.
                if f[1] == "d" || f[1] == "b" {
                    private.push(comp.clone());
                }
            }
        }
        let _ = saw;
    }
    if !private.is_empty() {
        return Err(format!(
            "components {private:?} carry a PRIVATE copy of std's allocator state \
             ({ALLOC_STATE} is a local symbol). Merged components share one linear \
             memory, so a private heap hands out memory another component already \
             owns — the module traps with `memory access out of bounds` on the first \
             allocation after a second component has allocated. Fat LTO does this; \
             `lto = \"thin\"` keeps the symbol global so first-wins gives the module \
             one allocator (D-H7-41)."
        ));
    }
    Ok(())
}

/// Runs roc with its output capped, as `build::roc_capped` does.
pub type RocRunner = dyn Fn(&[&str], &Path, &str) -> Result<(), String>;

/// Step 5: scan the wasm archives, then check and link the app.
pub fn link_app(
    dir: &Path,
    gen: &Path,
    world_file: &str,
    work: &Path,
    app: Option<&str>,
    out: &str,
    roc_capped: &RocRunner,
) -> Result<(), String> {
    crate::scan::scan_archives(dir, world_file, work, Format::Wasm)?;
    let Some(app) = app else {
        eprintln!("trantor build: wasm32 platform staged and scanned (no app)");
        return Ok(());
    };
    // `--app` names a directory holding main.roc, or a .roc file directly — an
    // app dir can carry one entry per world (`main.roc`, `dom.roc`) sharing
    // its modules, since a Roc app names exactly one platform.
    let app_main = if app.ends_with(".roc") { app.to_string() } else { format!("{app}/main.roc") };
    roc_capped(&["check", &app_main], dir, "roc check")?;
    let bin = gen.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| format!("mkdir {}: {e}", bin.display()))?;
    let bin = bin.canonicalize().map_err(|e| format!("canonicalize {}: {e}", bin.display()))?;
    let out_flag = format!("--output={}/{out}.wasm", bin.display());
    roc_capped(&["build", "--target=wasm32", &out_flag, &app_main], dir, "roc build (wasm32)")?;
    eprintln!("trantor build: linked {}/{out}.wasm", bin.display());
    Ok(())
}
