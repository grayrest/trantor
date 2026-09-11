//! Publishing a baseline platform, and classifying an extension's tier.
//!
//! A published baseline (D10/D11) is: the platform's `.roc` sources, the
//! prebuilt per-target archives, and a lock file carrying an **ABI fingerprint**
//! — a hash over the roc compiler, the glue spec, and the platform sources. A
//! Tier-1 consumer (pure-Roc extension) reuses the prebuilt archives and only
//! has to check that its compiler still matches the fingerprint (H11): if it
//! does, the prebuilt `libhost` is sound; if not, trantor warns rather than let
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

fn hash_file(path: &Path, h: u64) -> Result<u64, String> {
    let b = std::fs::read(path)
        .map_err(|e| format!("ABI fingerprint: read {}: {e}", path.display()))?;
    Ok(fnv1a(&b, h))
}

/// Fingerprint what determines `libhost`'s ABI: the roc compiler binary's
/// (size,mtime) and the glue spec. It deliberately does NOT hash the platform
/// `.roc` sources — a Tier-1 pure-Roc addition (a new module + an `exposes`
/// edit) changes those but adds no hosted symbols, so the prebuilt archives
/// stay valid and the fingerprint must stay stable.
///
/// **Machine-local by construction** (path, size and mtime are all local), so
/// it answers "did my toolchain move since I published?" and NOT "is this
/// baseline's toolchain the same as mine?". Making it portable is Track B
/// (D-U1-5), deferred with the rest of prebuilt baselines (D-U1-9).
///
/// Every input is mandatory (D-U1-10). It used to swallow both failures —
/// `hash_file` returned the accumulator unchanged on a read error and the roc
/// block sat inside `if let Ok(md)` — so a machine with neither roc nor glue
/// produced the bare FNV offset basis `cbf29ce484222325`, a plausible-looking
/// hex string IDENTICAL ON EVERY MACHINE. A fingerprint that agrees with
/// everything is worse than none: it is the stale-glue segfault this exists to
/// prevent, wearing the costume of a passing check.
pub fn abi_fingerprint(_dir: &Path) -> Result<String, String> {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let home = std::env::var("HOME")
        .map_err(|_| "ABI fingerprint: no $HOME and no $ROC to locate the roc compiler".to_string())?;
    let roc = std::env::var("ROC").unwrap_or_else(|_| format!("{home}/.bin/roc"));
    let md = std::fs::metadata(&roc)
        .map_err(|e| format!("ABI fingerprint: stat roc at {roc}: {e} (set $ROC)"))?;
    h = fnv1a(roc.as_bytes(), h);
    h = fnv1a(&md.len().to_le_bytes(), h);
    let mtime = md
        .modified()
        .map_err(|e| format!("ABI fingerprint: mtime of {roc}: {e}"))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("ABI fingerprint: mtime of {roc} predates the epoch: {e}"))?;
    h = fnv1a(&mtime.as_secs().to_le_bytes(), h);
    let glue = std::env::var("GLUE").unwrap_or_else(|_| format!("{home}/.bin/RustGlue.roc"));
    h = hash_file(Path::new(&glue), h)?;
    Ok(format!("{h:016x}"))
}

/// Publish the composed platform to `<dir>/dist/`: platform sources, prebuilt
/// archives, and `baseline.lock` with the fingerprint.
pub fn publish(dir: &Path) -> Result<(), String> {
    // The composed platform is generated, so it is read from
    // `target/trantor/<world>` (D-H7-38); `dist/` is an artifact too and goes
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
    //
    // KNOWN INCONSISTENCY, recorded not fixed (D-U1-10). The published
    // `main.roc` is copied verbatim, and it still lists the skipped archive in
    // `inputs:` and still binds its hosted symbols — so a baseline with any
    // `test_only` component declares a file this function deliberately deleted
    // and cannot link. Measured on b8-basic-cli: `libtestnet_host.a` in both
    // targets' `inputs`, `trantor__testnet_host__start_test_server` in the
    // hosted block, 12 archives shipped and that not one of them.
    // `b8-basic-cli/verify.sh` asserts the archive is ABSENT and calls that a
    // pass. Two-component has no test_only component, which is why
    // `verify-tier.sh` builds a Tier-1 app from its dist and stays green.
    //
    // Fixing it needs a Track B decision: strip the component from
    // `archive_order` and the hosted block too — and the published platform
    // then differs from the one that was tested — or drop `test_only` and move
    // test scaffolding into an overlay world. Deferred with D-U1-9.
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
        format!("# trantor baseline lock (D11/H11)\nabi_fingerprint = \"{fp}\"\n"),
    )
    .map_err(|e| e.to_string())?;
    eprintln!("trantor: published baseline to {} (abi_fingerprint {fp})", dist.display());
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
///
/// An extension manifest is deliberately a PARTIAL world — `two-component`'s
/// `extensions/tier1.toml` names `driver = "cli"` without declaring it, because
/// the driver belongs to the baseline being extended and the manifest lists
/// only what is ADDED. So this must not resolve the world; classification is a
/// question about the delta, not about a composable whole.
///
/// It must, however, refuse an EMPTY delta (D-U1-10). With no components at all
/// `host.is_empty()` was true and the answer was a confident "Tier 1: reuses the
/// baseline's prebuilt archives" about a world that adds nothing and was never
/// built — and a world scaffolded by `trantor new` has exactly that shape. The
/// count is printed for the same reason: a check reports the size of what it
/// examined.
pub fn classify(world: &World) -> Result<(Tier, usize), String> {
    let n = world.components.len();
    if n == 0 {
        return Err(
            "tier: this world declares no components, so there is no extension to classify. \
             An extension manifest lists what it ADDS over a baseline (see \
             tests/golden/two-component/extensions/tier1.toml); an empty one answers nothing."
                .to_string(),
        );
    }
    let host: Vec<String> = world
        .components
        .iter()
        .filter(|(_, c)| c.kind == "host" || c.kind == "driver")
        .map(|(n, _)| n.clone())
        .collect();
    Ok((if host.is_empty() { Tier::One } else { Tier::Two(host) }, n))
}
