//! Archive symbol-collision scan (H0c). After `cargo` builds the component
//! archives, `trantor scan <world-dir>` runs `nm` on each and rejects any
//! symbol that two components both DEFINE at global scope.
//!
//! Why: the roc link pulls component archives in order; a symbol defined in more
//! than one is resolved silently by archive order (first-in wins, later copies
//! never loaded) — no diagnostic. That is benign when the copies are identical
//! and memory-unsafe when they are not (two vendored sqlites with different
//! struct layouts sharing one implementation). The linker will not catch it, so
//! trantor must. See `notes/2026-09-04-h0-link-shape.md` (R2 / H0c).
//!
//! The exemptions are measured against the real archives, not guessed:
//!   - **Rust-mangled names** (`__R…` v0, `__Z…` legacy): ODR monomorphizations.
//!     The v0 mangling embeds a per-crate disambiguator hash, so a shared name
//!     means byte-identical code — safe to coalesce. On this toolchain LLVM
//!     emits them as PLAIN `external`, not weak, so the weak flag alone cannot
//!     separate them from a real C-symbol clash; the mangling prefix can.
//!   - **non-plain-external definitions**: `private external` and `weak external`
//!     (compiler-builtins softfloat, LSE atomics, libunwind, `_memcpy`) are
//!     hidden or coalesced and cannot first-wins-collide at global scope.
//!   - the **roc runtime** symbols (single-provider by construction).
//!   - names the world declares shared via `[world].shared_symbols` (globs).
//!
//! What remains after those is exactly the footgun: an unmangled, plain-external
//! symbol (a vendored native's `sqlite3_*`, or a hand-written host `#[no_mangle]`)
//! defined by two components. Empirically, a clean sole-vendor world scans to an
//! empty collision set.
//!
//! **One measured exception to the Rust-mangled rule (P0 of the H7 plan,
//! D-H7-13):** the allocator shims `__rust_alloc` / `__rust_dealloc` /
//! `__rust_realloc` are v0-mangled and plain-external in EVERY Rust archive
//! with one shared name, so the link first-wins them — the whole binary uses
//! whichever archive's shim is scanned first, and that decides whether a
//! `#[global_allocator]` takes effect. They are not ODR-identical when one
//! archive sets an allocator. trantor owns this by construction rather than
//! by report: `resolve` links the driver archive first (its allocator is the
//! binary's), and this scan refuses a `#[global_allocator]` in any other
//! component.
//!
//! wasm32 archives (D-H7-9) are read through `llvm-readobj` flags, and a Linux
//! host's ELF archives through `readelf`, instead of macho `nm -m`; the readers
//! live in `symbols.rs` and apply the same classification (a real definition,
//! neither hidden/private nor weak).

use crate::manifest::{component_dir, World};
use crate::resolve::sanitize;
pub use crate::symbols::Format;
use crate::symbols::{archive_defs, is_rust_mangled};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The attribute that must live in the driver only (D-H7-13).
const GLOBAL_ALLOCATOR_ATTR: &str = "#[global_allocator]";

/// roc runtime symbols (source names, no leading `_`). Provided by exactly one
/// component (the `provides_runtime` driver), enforced statically in resolve, so
/// these never actually collide — listed as belt-and-suspenders and to document
/// that a lone definition of each is legitimate.
const ROC_RUNTIME: &[&str] = &[
    "roc_alloc",
    "roc_realloc",
    "roc_dealloc",
    "roc_panic",
    "roc_dbg",
    "roc_crashed",
    "roc_expect_failed",
    "roc_memset",
    "roc_memcpy",
    "roc_shm_open",
    "roc_mmap",
    "roc_getppid",
];

/// One symbol defined by two or more components — a rejected collision.
struct Collision {
    symbol: String,        // as `nm` reports it (leading `_` included)
    components: Vec<String>, // component names that define it, link order
}

/// Rust runtime singletons that every Rust archive defines identically as a
/// plain global on wasm32 (`panic=abort` std still emits the personality;
/// on macho it is only referenced). First-wins by design, like the allocator
/// shims — one std, many archives — and never a vendored-native clash.
const RUST_RUNTIME_SINGLETONS: &[&str] = &["rust_eh_personality"];

/// Match a source symbol name (no leading `_`) against a `shared_symbols` entry:
/// a trailing `*` is a prefix glob, otherwise an exact match.
fn matches_shared(sym: &str, pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => sym.starts_with(prefix),
        None => sym == pattern,
    }
}

/// Scan every host/driver component archive of a composed world for global
/// symbol collisions (H0c). `targets_dir` is the directory holding the
/// per-target archive dirs (default `<dir>/platform/targets`); `target` names
/// the archive subdir (default: the host's roc target).
pub fn scan(
    dir: &Path,
    world_file: &str,
    targets_dir: Option<PathBuf>,
    target: &str,
) -> Result<(), String> {
    let arch_dir = match targets_dir {
        Some(d) => d,
        // Generated, so under `target/trantor/<world>` (D-H7-38).
        None => crate::manifest::out_dir(dir, &crate::manifest::load_world(dir, world_file)?)
            .join("platform")
            .join("targets"),
    }
    .join(target);
    scan_archives(dir, world_file, &arch_dir, Format::native())
}

/// Refuse a `#[global_allocator]` outside the driver (D-H7-13): the allocator
/// shims are first-wins across the link and the driver's archive is linked
/// first, so any other component's allocator would be silently ignored.
fn check_global_allocator(dir: &Path, world: &World) -> Result<(), String> {
    let mut examined = 0usize;
    let mut hosts = 0usize;
    for (name, c) in &world.components {
        if c.kind != "host" {
            continue;
        }
        hosts += 1;
        let src = component_dir(dir, name, c).join("src");
        // `rust_files` returns nothing for a directory it cannot read, and an
        // empty set reads as "clean" — so a component whose sources are not
        // where we looked would sail through this guard rather than trip it.
        // That is the whole silent-check pattern, and it is live now that a
        // dependency's crates sit outside the world (D-U1-1): if expansion ever
        // fails to rewrite a path, every dependency's `#[global_allocator]`
        // becomes invisible instead of rejected.
        let files = rust_files(&src);
        if files.is_empty() {
            return Err(format!(
                "component `{name}`: no Rust sources under {} — the allocator guard cannot \
                 examine it, and an unexaminable component must not pass for a clean one \
                 (D-H7-13).",
                src.display()
            ));
        }
        examined += files.len();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            if text.contains(GLOBAL_ALLOCATOR_ATTR) {
                return Err(format!(
                    "component `{name}` sets `{GLOBAL_ALLOCATOR_ATTR}` ({}): the allocator shims are \
                     first-wins across the link and the driver's archive is linked first, so this \
                     allocator would be silently ignored. Only the driver may set one (D-H7-13).",
                    file.display()
                ));
            }
        }
    }
    if hosts > 0 {
        eprintln!("trantor: allocator guard examined {examined} file(s) across {hosts} host component(s)");
    }
    Ok(())
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(rust_files(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

/// Scan the archives in `arch_dir` (one `lib<sanitized>.a` per non-Roc
/// component) in the given symbol format.
pub fn scan_archives(dir: &Path, world_file: &str, arch_dir: &Path, format: Format) -> Result<(), String> {
    let world: World = crate::manifest::load_world(dir, world_file)?;
    check_global_allocator(dir, &world)?;

    // The linked archives: every non-Roc component (host + driver + test-only).
    // A pure-Roc component ships no archive. cargo names each `lib<sanitized>.a`
    // (it replaces non-identifier chars the same way `sanitize` does).
    let mut components: Vec<(String, PathBuf)> = Vec::new();
    for (name, comp) in &world.components {
        if comp.kind == "roc" {
            continue;
        }
        let archive = arch_dir.join(format!("lib{}.a", sanitize(name)));
        if !archive.exists() {
            return Err(format!(
                "component `{name}`: archive {} not found — run the world's build first",
                archive.display()
            ));
        }
        components.push((name.clone(), archive));
    }
    if components.len() < 2 {
        // Nothing can collide with fewer than two archives.
        eprintln!(
            "trantor: scan `{}` — {} archive(s), no collision possible",
            world.world.name,
            components.len()
        );
        return Ok(());
    }

    // symbol -> components that define it plain-external (excluding exempt classes).
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let runtime: BTreeSet<&str> = ROC_RUNTIME.iter().copied().collect();
    for (name, archive) in &components {
        for sym in archive_defs(archive, format)? {
            if is_rust_mangled(&sym, format) {
                continue;
            }
            let source = sym.strip_prefix('_').unwrap_or(&sym);
            if runtime.contains(source) || RUST_RUNTIME_SINGLETONS.contains(&source) {
                continue;
            }
            owners.entry(sym.clone()).or_default().push(name.clone());
        }
    }

    // A collision is a symbol owned by >1 component and not declared shared.
    let mut collisions: Vec<Collision> = Vec::new();
    for (symbol, comps) in owners {
        if comps.len() < 2 {
            continue;
        }
        let source = symbol.strip_prefix('_').unwrap_or(&symbol);
        if world.world.shared_symbols.iter().any(|p| matches_shared(source, p)) {
            continue;
        }
        collisions.push(Collision { symbol, components: comps });
    }

    if collisions.is_empty() {
        eprintln!(
            "trantor: scan `{}` clean — {} archives, no global symbol collisions",
            world.world.name,
            components.len()
        );
        return Ok(());
    }

    let mut msg = format!(
        "H0c collision: {} symbol(s) defined by more than one component in world `{}`.\n\
         The linker resolves each silently by archive order (first-in wins), which is \
         memory-unsafe if the definitions differ. Deduplicate to one component, isolate \
         the symbols, split the worlds, or — if the copies are genuinely one shared \
         native — declare it in [world].shared_symbols.\n",
        collisions.len(),
        world.world.name
    );
    for c in &collisions {
        msg.push_str(&format!("  {} — defined by {:?}\n", c.symbol, c.components));
    }
    Err(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_allocator_outside_the_driver_is_refused() {
        let dir = std::env::temp_dir().join(format!("trantor-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("components/svc/src")).unwrap();
        std::fs::create_dir_all(dir.join("components/drv/src")).unwrap();
        std::fs::write(dir.join("components/drv/src/lib.rs"), "#[global_allocator]\nstatic A: X = X;\n").unwrap();
        std::fs::write(dir.join("components/svc/src/lib.rs"), "fn f() {}\n").unwrap();
        let world: World = toml::from_str(
            "[world]\nname = \"w\"\ndriver = \"drv\"\n[components.drv]\nkind = \"driver\"\n\
             [components.svc]\nkind = \"host\"\n[wiring]\n",
        )
        .unwrap();
        assert!(check_global_allocator(&dir, &world).is_ok(), "the driver may set one");
        std::fs::write(dir.join("components/svc/src/lib.rs"), "#[global_allocator]\nstatic A: X = X;\n").unwrap();
        let err = check_global_allocator(&dir, &world).unwrap_err();
        assert!(err.contains("`svc`") && err.contains("D-H7-13"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shared_glob_matching() {
        assert!(matches_shared("sqlite3_open", "sqlite3_*"));
        assert!(matches_shared("sqlite3_open", "sqlite3_open"));
        assert!(!matches_shared("sqlite3_open", "sqlite3_close"));
        assert!(!matches_shared("other", "sqlite3_*"));
    }
}
