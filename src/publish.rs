//! Publishing a baseline platform, and classifying an extension's tier.
//!
//! A published baseline (D10/D11) is: the platform's `.roc` sources, the
//! prebuilt per-target archives, and a lock file carrying an **ABI fingerprint**
//! — a hash over the roc compiler, the glue spec, and the platform sources. A
//! Tier-1 consumer (pure-Roc extension) reuses the prebuilt archives and only
//! has to check that its compiler still matches the fingerprint (H11): if it
//! does, the prebuilt `libhost` is sound; if not, hematite warns rather than let
//! a stale-glue mismatch become a runtime segfault.

use crate::manifest::World;
use std::path::Path;

/// A cheap, stable content hash (FNV-1a over bytes). Not cryptographic — enough
/// to detect that an input changed. The upstream glue redesign's real ABI
/// fingerprint (roc_abi_assert!) supersedes this at link time; this is the
/// distribution-time check the baseline tarball carries.
fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn hash_file(path: &Path, h: u64) -> u64 {
    match std::fs::read(path) {
        Ok(b) => fnv1a(&b, h),
        Err(_) => h,
    }
}

/// Fingerprint what determines `libhost`'s ABI: the roc compiler binary's
/// (size,mtime) and the glue spec. It deliberately does NOT hash the platform
/// `.roc` sources — a Tier-1 pure-Roc addition (a new module + an `exposes`
/// edit) changes those but adds no hosted symbols, so the prebuilt archives
/// stay valid and the fingerprint must stay stable. The check a Tier-1 consumer
/// runs is: does my compiler+glue match the baseline's? If yes, the prebuilt
/// `libhost` was glued against a compatible ABI and can be reused without cargo
/// or glue (H11). The `_dir` argument is kept for a future per-hosted-surface
/// hash if a stricter check is wanted.
pub fn abi_fingerprint(_dir: &Path) -> Result<String, String> {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    if let Ok(roc) = std::env::var("ROC").or_else(|_| {
        std::env::var("HOME").map(|home| format!("{home}/.bin/roc"))
    }) {
        if let Ok(md) = std::fs::metadata(&roc) {
            h = fnv1a(roc.as_bytes(), h);
            h = fnv1a(&md.len().to_le_bytes(), h);
            if let Ok(mtime) = md.modified() {
                if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                    h = fnv1a(&d.as_secs().to_le_bytes(), h);
                }
            }
        }
    }
    if let Ok(glue) = std::env::var("GLUE").or_else(|_| {
        std::env::var("HOME").map(|home| format!("{home}/.bin/RustGlue.roc"))
    }) {
        h = hash_file(Path::new(&glue), h);
    }
    Ok(format!("{h:016x}"))
}

/// Publish the composed platform to `<dir>/dist/`: platform sources, prebuilt
/// archives, and `baseline.lock` with the fingerprint.
pub fn publish(dir: &Path) -> Result<(), String> {
    // The composed platform is generated, so it is read from
    // `target/hematite/<world>` (D-H7-38); `dist/` is an artifact too and goes
    // beside it.
    let gen = match crate::manifest::load_world(dir, "world.toml") {
        Ok(w) => crate::manifest::out_dir(dir, &w),
        Err(e) => return Err(e),
    };
    let dist = gen.join("dist");
    let _ = std::fs::remove_dir_all(&dist);
    std::fs::create_dir_all(dist.join("platform")).map_err(|e| e.to_string())?;

    // copy platform .roc sources
    let pdir = gen.join("platform");
    for e in std::fs::read_dir(&pdir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        if p.extension().map_or(false, |x| x == "roc") {
            std::fs::copy(&p, dist.join("platform").join(p.file_name().unwrap()))
                .map_err(|e| e.to_string())?;
        }
    }
    // Archives of test-only components (e.g. a testnet server) are linked into
    // the app but MUST NOT ship in the baseline: collect their file names to skip.
    let test_archives: std::collections::BTreeSet<String> =
        match crate::manifest::load_world(dir, "world.toml") {
            Ok(world) => world
                .components
                .iter()
                .filter(|(_, c)| c.test_only)
                .map(|(n, _)| format!("lib{}.a", crate::resolve::sanitize(n)))
                .collect(),
            Err(_) => std::collections::BTreeSet::new(),
        };
    // copy prebuilt archives per target
    let tdir = pdir.join("targets");
    if let Ok(targets) = std::fs::read_dir(&tdir) {
        for t in targets.flatten() {
            if t.path().is_dir() {
                let name = t.file_name();
                let dest = dist.join("platform/targets").join(&name);
                std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
                for a in std::fs::read_dir(t.path()).map_err(|e| e.to_string())?.flatten() {
                    let ap = a.path();
                    let is_test = ap.file_name().and_then(|f| f.to_str()).map_or(false, |f| test_archives.contains(f));
                    if ap.extension().map_or(false, |x| x == "a") && !is_test {
                        std::fs::copy(&ap, dest.join(ap.file_name().unwrap()))
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    let fp = abi_fingerprint(dir)?;
    std::fs::write(
        dist.join("baseline.lock"),
        format!("# hematite baseline lock (D11/H11)\nabi_fingerprint = \"{fp}\"\n"),
    )
    .map_err(|e| e.to_string())?;
    eprintln!("hematite: published baseline to {} (abi_fingerprint {fp})", dist.display());
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum Tier {
    /// Pure-Roc only: no new hosted symbols, prebuilt archives stay valid.
    One,
    /// Adds host code: full source composition (cargo + glue) required.
    Two(Vec<String>),
}

/// Classify a world's components into the extension tier. A world that adds any
/// `kind = "host"`/`"driver"` component beyond a pure-Roc set is Tier 2.
pub fn classify(world: &World) -> Tier {
    let host: Vec<String> = world
        .components
        .iter()
        .filter(|(_, c)| c.kind == "host" || c.kind == "driver")
        .map(|(n, _)| n.clone())
        .collect();
    if host.is_empty() {
        Tier::One
    } else {
        Tier::Two(host)
    }
}
