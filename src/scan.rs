//! Archive symbol-collision scan (H0c). After `cargo` builds the component
//! archives, `hematite scan <world-dir>` runs `nm` on each and rejects any
//! symbol that two components both DEFINE at global scope.
//!
//! Why: the roc link pulls component archives in order; a symbol defined in more
//! than one is resolved silently by archive order (first-in wins, later copies
//! never loaded) — no diagnostic. That is benign when the copies are identical
//! and memory-unsafe when they are not (two vendored sqlites with different
//! struct layouts sharing one implementation). The linker will not catch it, so
//! hematite must. See `notes/2026-09-04-h0-link-shape.md` (R2 / H0c).
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

use crate::manifest::World;
use crate::resolve::sanitize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Is this the leading-underscore macho form of a Rust-mangled symbol? v0 is
/// `_R…` (macho `__R…`); the legacy Itanium form is `_ZN…` (macho `__Z…`).
fn is_rust_mangled(nm_name: &str) -> bool {
    nm_name.starts_with("__R") || nm_name.starts_with("__Z")
}

/// Match a source symbol name (no leading `_`) against a `shared_symbols` entry:
/// a trailing `*` is a prefix glob, otherwise an exact match.
fn matches_shared(sym: &str, pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => sym.starts_with(prefix),
        None => sym == pattern,
    }
}

/// Parse `nm -m` output, returning the set of PLAIN-external DEFINED symbol names
/// (leading `_` kept). `nm -m` prints one symbol per line as
/// `<addr> (<section>) <attrs> [annotations…] <name>`; a definition has a real
/// section (not `undefined`) and the attribute token immediately after `)` is
/// exactly `external` (not `private external`, `weak external`,
/// `weak private external`, or `non-external`). Between the attrs and the name
/// nm may insert bracketed annotations (`[cold func]`, `[referenced dynamically]`,
/// …), so the name is taken as the last whitespace token — never the text right
/// after `external `.
fn plain_external_defs(nm_output: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in nm_output.lines() {
        // Split once on ") " into `<addr> (<section>` and `<attrs> … <name>`.
        let Some((head, rest)) = line.split_once(") ") else {
            continue;
        };
        // Section is what follows the last '(' in head.
        let Some(section) = head.rsplit('(').next() else {
            continue;
        };
        if section == "undefined" {
            continue; // a reference, not a definition
        }
        // A plain-external definition's attribute string starts with "external ";
        // "weak external", "private external", "weak private external" and
        // "non-external" all begin with a different word.
        if !rest.starts_with("external ") {
            continue;
        }
        // The symbol name is the final token (symbols carry no spaces); this
        // skips any `[cold func]`-style annotation nm places before it.
        if let Some(name) = rest.split_whitespace().next_back() {
            out.insert(name.to_string());
        }
    }
    out
}

/// Run `nm -m` on one archive. Xcode's `nm` exits nonzero on rust-LLVM archives
/// it can only partially read yet still prints the symbols it can, so the exit
/// status is ignored and stdout is used regardless (matching the fixtures'
/// `{ nm … || true; }` idiom).
fn nm_defs(archive: &Path) -> Result<BTreeSet<String>, String> {
    let out = Command::new("nm")
        .arg("-m")
        .arg(archive)
        .output()
        .map_err(|e| format!("run nm on {}: {e}", archive.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let defs = plain_external_defs(&text);
    if defs.is_empty() {
        return Err(format!(
            "nm read no defined symbols from {} (build the world first?)",
            archive.display()
        ));
    }
    Ok(defs)
}

/// Scan every host/driver component archive of a composed world for global
/// symbol collisions (H0c). `targets_dir` is the directory holding the
/// per-target archive dirs (default `<dir>/platform/targets`); `target` names
/// the archive subdir (default `arm64mac`).
pub fn scan(
    dir: &Path,
    world_file: &str,
    targets_dir: Option<PathBuf>,
    target: &str,
) -> Result<(), String> {
    let world: World = crate::manifest::load_world(dir, world_file)?;

    // The linked archives: every non-Roc component (host + driver + test-only).
    // A pure-Roc component ships no archive. cargo names each `lib<sanitized>.a`
    // (it replaces non-identifier chars the same way `sanitize` does).
    let arch_dir = targets_dir
        .unwrap_or_else(|| dir.join("platform").join("targets"))
        .join(target);
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
            "hematite: scan `{}` — {} archive(s), no collision possible",
            world.world.name,
            components.len()
        );
        return Ok(());
    }

    // symbol -> components that define it plain-external (excluding exempt classes).
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let runtime: BTreeSet<&str> = ROC_RUNTIME.iter().copied().collect();
    for (name, archive) in &components {
        for sym in nm_defs(archive)? {
            if is_rust_mangled(&sym) {
                continue;
            }
            let source = sym.strip_prefix('_').unwrap_or(&sym);
            if runtime.contains(source) {
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
            "hematite: scan `{}` clean — {} archives, no global symbol collisions",
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
    fn parses_plain_external_defs_only() {
        let sample = "\
                     (undefined) external _sqlite3_step\n\
0000000000009004 (__TEXT,__text) external _sqlite3_open\n\
0000000000000000 (__TEXT,__text) private external __aarch64_cas8_acq\n\
---------------- (LTO,CODE) weak private external ___multi3\n\
0000000000000600 (__TEXT,__text) external __RNvMs_NtCsuFXAkltCeT_12hematite_abi9RocStr8from_str\n\
0000000000000800 (__TEXT,__text_cold) external [cold func] __RINvNtCsX_4core9panicking13assert_failed_rustls\n\
0000000000000010 (__TEXT,__text) non-external _local_helper\n";
        let defs = plain_external_defs(sample);
        assert!(defs.contains("_sqlite3_open")); // plain external, defined
        assert!(defs.contains("__RNvMs_NtCsuFXAkltCeT_12hematite_abi9RocStr8from_str"));
        // annotation before the name: the mangled name, not "[cold func]", is captured
        assert!(defs.contains("__RINvNtCsX_4core9panicking13assert_failed_rustls"));
        assert!(!defs.contains("_sqlite3_step")); // undefined reference
        assert!(!defs.contains("__aarch64_cas8_acq")); // private external
        assert!(!defs.contains("___multi3")); // weak private external
        assert!(!defs.contains("_local_helper")); // non-external
    }

    #[test]
    fn rust_mangled_recognized() {
        assert!(is_rust_mangled("__RNvMs_NtCs_12hematite_abi9RocStr8from_str"));
        assert!(is_rust_mangled("__ZN4core3fmt3num"));
        assert!(!is_rust_mangled("_sqlite3_open"));
        assert!(!is_rust_mangled("_vendored_answer"));
    }

    #[test]
    fn shared_glob_matching() {
        assert!(matches_shared("sqlite3_open", "sqlite3_*"));
        assert!(matches_shared("sqlite3_open", "sqlite3_open"));
        assert!(!matches_shared("sqlite3_open", "sqlite3_close"));
        assert!(!matches_shared("other", "sqlite3_*"));
    }
}
