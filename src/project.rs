//! The project `add`, `update` and `remove` edit: a world or a package, and
//! the rule they share — the edit is kept only if the project still composes
//! (D-U1-3, T3b). The old bytes are journaled first, so neither a failed check
//! nor a killed trantor leaves the manifest or the lock half changed.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::journal::Journal;
use crate::manifest::{Dep, Package};
use crate::package_worlds::{self, Deps, Reach};
use crate::registry::LOCK_FILE;

pub enum Target {
    World(String),
    Package,
}

impl Target {
    /// An explicit `--world` wins; then a package here; then a world here.
    /// Package first, as `trantor test` decides it (D-T3-9): a world.toml
    /// beside a package.toml is a stray, not the project.
    pub fn of(dir: &Path, world_flag: Option<&str>) -> Result<Target, String> {
        if let Some(w) = world_flag {
            return Ok(Target::World(w.to_string()));
        }
        if dir.join("package.toml").is_file() {
            return Ok(Target::Package);
        }
        if dir.join("world.toml").is_file() {
            return Ok(Target::World("world.toml".into()));
        }
        if !dir.is_dir() {
            return Err(format!("{} does not exist", dir.display()));
        }
        Err(format!("{} has neither a world.toml nor a package.toml", dir.display()))
    }

    pub fn manifest(&self) -> &str {
        match self {
            Target::World(w) => w,
            Target::Package => "package.toml",
        }
    }

    pub fn deps(&self, dir: &Path) -> Result<BTreeMap<String, Dep>, String> {
        match self {
            Target::World(w) => Ok(crate::manifest::load_world_raw(dir, w)?.deps),
            // A test baseline is pinned like any other dependency.
            Target::Package => {
                let pkg = read_package(dir)?;
                Ok(pkg.dev_deps.into_iter().chain(pkg.deps).collect())
            }
        }
    }

    /// Names every OTHER manifest in the directory depends on: they share the
    /// lock, so a pin one of them uses is not this edit's to drop.
    pub fn names_used_elsewhere(&self, dir: &Path) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for (file, deps) in world_variants(dir) {
            if !matches!(self, Target::World(w) if same_manifest(dir, w, &file)) {
                names.extend(deps.into_keys());
            }
        }
        if !matches!(self, Target::Package) {
            if let Ok(deps) = Target::Package.deps(dir) {
                names.extend(deps.into_keys());
            }
        }
        names
    }

    /// The edited world composes, and so does every other world variant in the
    /// directory that has github deps — they share the lock this edit changed.
    fn validate(&self, dir: &Path) -> Result<(), String> {
        let Target::World(w) = self else { return validate_package(dir) };
        crate::build::compose(dir, w, None)?;
        for (file, deps) in world_variants(dir) {
            let pinned = deps.iter().any(|(n, d)| matches!(d.source(n), Ok(crate::manifest::DepSource::GitHub(_))));
            if pinned && !same_manifest(dir, w, &file) {
                crate::build::compose(dir, &file, None).map_err(|e| format!("{file} shares {LOCK_FILE} and no longer composes: {e}"))?;
            }
        }
        Ok(())
    }
}

/// Every world manifest directly in `dir`, with its dependencies.
fn world_variants(dir: &Path) -> Vec<(String, BTreeMap<String, Dep>)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<(String, BTreeMap<String, Dep>)> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".toml") && !matches!(n.as_str(), "package.toml" | "Cargo.toml"))
        .filter_map(|n| crate::manifest::load_world_raw(dir, &n).ok().map(|w| (n, w.deps)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn same_manifest(dir: &Path, a: &str, b: &str) -> bool {
    crate::package_modules::same_file(&dir.join(a), &dir.join(b))
}

/// Run `edit`, which writes the manifest and the lock and says whether
/// anything changed, then compose. Anything short of success restores both.
pub fn transact(dir: &Path, target: &Target, edit: impl FnOnce() -> Result<bool, String>) -> Result<bool, String> {
    let journal = Journal::begin(dir, &[target.manifest(), LOCK_FILE])?;
    let checked = edit().and_then(|changed| {
        journal.written()?;
        if changed { target.validate(dir).map(|()| true) } else { Ok(false) }
    });
    match checked {
        Ok(changed) => {
            journal.commit();
            Ok(changed)
        }
        Err(e) => {
            let note = match journal.rollback() {
                Ok(()) => format!("{} and {LOCK_FILE} are unchanged", target.manifest()),
                Err(r) => format!("AND restoring the previous files failed: {r}"),
            };
            Err(format!("{e}\n({note})"))
        }
    }
}

fn read_package(dir: &Path) -> Result<Package, String> {
    let p = dir.join("package.toml");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}

/// A package is composed the way `trantor test` composes it: on its
/// [dev-deps], or on the driver its [deps] reach, with its lock beside it.
/// One with neither cannot be composed, and a change to it cannot be checked —
/// expanding the dependencies instead let a package that fails to compose be
/// added (T3b-2), so it is refused with what to add.
fn validate_package(dir: &Path) -> Result<(), String> {
    let root = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let pkg = read_package(&root)?;
    if pkg.dev_deps.is_empty() {
        if let Reach::No = package_worlds::driver_in_reach(&root, &pkg, 0) {
            return Err(format!(
                "`{}` has no [dev-deps] and nothing in its [deps] provides a driver, so it cannot be \
                 composed to check this change. Name the baseline it is used on under [dev-deps] \
                 (e.g. `base = {{ github = \"<org>/<repo>\" }}`), `trantor update` to pin it, and run this again",
                pkg.package.name
            ));
        }
    }
    let scratch = Scratch(package_worlds::fresh_scratch_named("trantor-add", &pkg.package.name)?);
    let world = package_worlds::scratch_world(&root, &pkg, &scratch.0, "check", Deps::WithPackage, &[])?;
    crate::build::compose(&world, "world.toml", None)
}

/// A directory removed however the check ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_world_wins_then_a_package_then_a_world() {
        let d = std::env::temp_dir().join(format!("trantor-target-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("world.toml"), "").unwrap();
        assert!(matches!(Target::of(&d, None).unwrap(), Target::World(w) if w == "world.toml"));
        std::fs::write(d.join("package.toml"), "").unwrap();
        assert!(matches!(Target::of(&d, None).unwrap(), Target::Package));
        assert!(matches!(Target::of(&d, Some("alt.toml")).unwrap(), Target::World(w) if w == "alt.toml"));
        std::fs::remove_dir_all(&d).ok();
    }
}
