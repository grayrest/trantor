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

    /// Names every OTHER manifest sharing this lock depends on, so a pin one
    /// of them uses is not this edit's to drop. A world that does not parse
    /// still counts: its `[deps]` are read as plain TOML.
    pub fn names_used_elsewhere(&self, dir: &Path) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for file in world_files(dir) {
            if !matches!(self, Target::World(w) if same_manifest(dir, w, &file)) {
                names.extend(dep_names(&dir.join(&file)));
            }
        }
        if !matches!(self, Target::Package) {
            names.extend(dep_names(&dir.join("package.toml")));
        }
        names
    }

    /// The edited project composes, and so does every world sharing the lock
    /// that depends on a name whose pin this edit changed. Those are composed
    /// into scratch output: composing a variant in place overwrote the edited
    /// world's platform under `target/`.
    fn validate(&self, dir: &Path, changed: &BTreeSet<String>) -> Result<(), String> {
        match self {
            Target::World(w) => crate::build::compose(dir, w, None)?,
            Target::Package => validate_package(dir)?,
        }
        for file in world_files(dir) {
            let uses: Vec<String> = dep_names(&dir.join(&file)).intersection(changed).cloned().collect();
            if uses.is_empty() || matches!(self, Target::World(w) if same_manifest(dir, w, &file)) {
                continue;
            }
            let out = Scratch(package_worlds::fresh_scratch_named("trantor-variant", &file.replace('/', "_"))?);
            crate::build::compose(dir, &file, Some(out.0.clone())).map_err(|e| {
                format!("{file} shares {LOCK_FILE}, depends on `{}` whose pin this changes, and does not compose with it \
                         (commands for that world take `--world {file}`): {e}", uses.join("`, `"))
            })?;
        }
        Ok(())
    }
}

/// World manifests sharing `dir`'s lock: every `*.toml` under it that has a
/// `[world]` table, outside `target/`, dot-directories, and directories that
/// are projects of their own (a `trantor.lock` or `package.toml` of theirs).
fn world_files(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                let own_project = path.join(LOCK_FILE).exists() || path.join("package.toml").exists();
                if !(name.starts_with('.') || name == "target" || own_project) {
                    walk(root, &path, out);
                }
            } else if name.ends_with(".toml") && !matches!(name.as_str(), "package.toml" | "Cargo.toml") {
                let is_world = std::fs::read_to_string(&path).ok().and_then(|t| t.parse::<toml::Table>().ok()).is_some_and(|t| t.contains_key("world"));
                if is_world {
                    out.push(path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned());
                }
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The names under `[deps]` and `[dev-deps]`, read as plain TOML.
fn dep_names(manifest: &Path) -> BTreeSet<String> {
    let Some(table) = std::fs::read_to_string(manifest).ok().and_then(|t| t.parse::<toml::Table>().ok()) else { return BTreeSet::new() };
    ["deps", "dev-deps"].iter().filter_map(|k| table.get(*k).and_then(|v| v.as_table())).flat_map(|t| t.keys().cloned()).collect()
}

fn same_manifest(dir: &Path, a: &str, b: &str) -> bool {
    crate::package_modules::same_file(&dir.join(a), &dir.join(b))
}

/// Run `edit`, which writes the manifest and the lock and says whether
/// anything changed, then compose. Anything short of success restores both.
pub fn transact(dir: &Path, target: &Target, edit: impl FnOnce() -> Result<bool, String>) -> Result<bool, String> {
    let journal = Journal::begin(dir, &[target.manifest(), LOCK_FILE])?;
    let pins_before = crate::registry::Lock::load(dir)?.packages;
    let checked = edit().and_then(|changed| {
        journal.written()?;
        if !changed {
            return Ok(false);
        }
        let after = crate::registry::Lock::load(dir)?.packages;
        let moved: BTreeSet<String> = pins_before.iter().filter(|e| !after.contains(e)).chain(after.iter().filter(|e| !pins_before.contains(e))).map(|e| e.name.clone()).collect();
        target.validate(dir, &moved).map(|()| true)
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
