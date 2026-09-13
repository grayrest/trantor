//! The project `add`, `update` and `remove` edit: a world or a package, and
//! the rule they share — the edit is kept only if the project still composes
//! (D-U1-3, T3b). The old bytes are journaled first, so neither a failed check
//! nor a killed trantor leaves the manifest or the lock half changed.
use std::collections::BTreeMap;
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
            Target::Package => Ok(read_package(dir)?.deps),
        }
    }

    fn validate(&self, dir: &Path) -> Result<(), String> {
        match self {
            Target::World(w) => crate::build::compose(dir, w, None),
            Target::Package => validate_package(dir),
        }
    }
}

/// Run `edit`, which writes the manifest and the lock and says whether
/// anything changed, then compose. Anything short of success restores both.
pub fn transact(dir: &Path, target: &Target, edit: impl FnOnce() -> Result<bool, String>) -> Result<bool, String> {
    let journal = Journal::begin(dir, &[target.manifest(), LOCK_FILE])?;
    match edit().and_then(|changed| if changed { target.validate(dir).map(|()| true) } else { Ok(false) }) {
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
    let scratch = Scratch(std::env::temp_dir().join(format!("trantor-add-{}-{}", pkg.package.name, std::process::id())));
    std::fs::remove_dir_all(&scratch.0).ok();
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
