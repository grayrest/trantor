//! `trantor add <org>/<repo> [<dir>]` (D-U1-3, T3b).
//!
//! Adds the dependency to the project in `<dir>` (default: here): its
//! `world.toml`, or — in a package — its `package.toml`, pinned in that
//! directory's `trantor.lock`. Then it composes, as the U1 plan always said it
//! did, and only a composition that succeeds keeps the edit: on any failure the
//! manifest and the lock are put back byte for byte, so a failed `add` changes
//! nothing.
use std::path::{Path, PathBuf};

use crate::manifest::Package;
use crate::registry::{self, LOCK_FILE};

pub fn add(dir: &Path, world_flag: Option<&str>, slug: &str, as_name: Option<&str>) -> Result<(), String> {
    let target = Target::of(dir, world_flag)?;
    let fetched = registry::fetch(slug)?;
    let dep_name = as_name.unwrap_or(&fetched.name).to_string();

    let manifest = Snapshot::take(&dir.join(target.manifest()))?;
    let lock = Snapshot::take(&dir.join(LOCK_FILE))?;
    let result = registry::record(dir, target.manifest(), &dep_name, &fetched)
        .and_then(|changed| target.validate(dir).map(|()| changed));
    match result {
        Ok(changed) => {
            let pin = fetched.tag.as_deref().map(|t| format!("tag {t}"))
                .unwrap_or_else(|| format!("{} HEAD (no semver tags)", fetched.branch.as_deref().unwrap_or("default branch")));
            eprintln!(
                "trantor: added `{dep_name}` = {slug} to {} at {pin}, commit {}{}",
                target.manifest(),
                &fetched.commit[..fetched.commit.len().min(12)],
                if changed { "" } else { " (lock unchanged)" }
            );
            Ok(())
        }
        Err(e) => {
            let restored = manifest.restore().and(lock.restore());
            let note = match restored {
                Ok(()) => format!("{} and {LOCK_FILE} are unchanged", target.manifest()),
                Err(r) => format!("AND restoring the previous files failed: {r}"),
            };
            Err(format!("add {slug}: {e}\n({note})"))
        }
    }
}

/// `trantor add <org>/<repo> [<dir>] [--as <name>] [--world <file>]`: the
/// dependency is the first positional, the project directory the optional
/// second (default: the current one).
pub fn command(it: &mut std::iter::Skip<std::slice::Iter<'_, String>>) -> Result<(), String> {
    let (mut positional, mut as_name, mut world): (Vec<String>, Option<String>, Option<String>) = (vec![], None, None);
    while let Some(f) = it.next() {
        match f.as_str() {
            "--as" => as_name = Some(it.next().ok_or("--as: missing name")?.clone()),
            "--world" => world = Some(it.next().ok_or("--world: missing file")?.clone()),
            other if other.starts_with("--") => return Err(format!("unknown flag {other:?}")),
            other => positional.push(other.to_string()),
        }
    }
    let (slug, dir) = match positional.as_slice() {
        [slug] => (slug.clone(), PathBuf::from(".")),
        [first, second] if !is_slug(first) && is_slug(second) => {
            return Err(format!("add: arguments are `trantor add <org>/<repo> [<dir>]` — try `trantor add {second} {first}`"));
        }
        [slug, dir] => (slug.clone(), PathBuf::from(dir)),
        [] => return Err("add: missing <org>/<repo>; usage: trantor add <org>/<repo> [<dir>]".into()),
        _ => return Err("add: expected `trantor add <org>/<repo> [<dir>]`".into()),
    };
    add(&dir, world.as_deref(), &slug, as_name.as_deref())
}

fn is_slug(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && !p.starts_with('.')) && !std::path::Path::new(s).exists()
}

enum Target {
    World(String),
    Package,
}

impl Target {
    /// An explicit `--world` wins; then a world here; then a package here.
    fn of(dir: &Path, world_flag: Option<&str>) -> Result<Target, String> {
        if let Some(w) = world_flag {
            return Ok(Target::World(w.to_string()));
        }
        if dir.join("world.toml").is_file() {
            return Ok(Target::World("world.toml".into()));
        }
        if dir.join("package.toml").is_file() {
            return Ok(Target::Package);
        }
        Err(format!("{} has neither a world.toml nor a package.toml to add to", dir.display()))
    }

    fn manifest(&self) -> &str {
        match self {
            Target::World(w) => w,
            Target::Package => "package.toml",
        }
    }

    fn validate(&self, dir: &Path) -> Result<(), String> {
        match self {
            Target::World(w) => crate::build::compose(dir, w, None),
            Target::Package => validate_package(dir),
        }
    }
}

/// A package cannot always compose alone — an add-on has no driver — so it is
/// composed the way `trantor test` composes it: on its [dev-deps], with its
/// lock beside it. With no dev-deps and no driver in reach, expanding its
/// dependencies is as far as it can be taken, and that still proves the new
/// one resolves, fetches and parses.
fn validate_package(dir: &Path) -> Result<(), String> {
    let root = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = std::fs::read_to_string(root.join("package.toml")).map_err(|e| format!("read package.toml: {e}"))?;
    let pkg: Package = toml::from_str(&text).map_err(|e| format!("parse package.toml: {e}"))?;
    let scratch = std::env::temp_dir().join(format!("trantor-add-{}-{}", pkg.package.name, std::process::id()));
    std::fs::remove_dir_all(&scratch).ok();
    let world = crate::package_worlds::scratch_world(&root, &pkg, &scratch, "check", crate::package_worlds::Deps::WithPackage, &[])?;
    let can_compose = !pkg.dev_deps.is_empty()
        || matches!(crate::package_worlds::driver_in_reach(&root, &pkg, 0), crate::package_worlds::Reach::Yes);
    let result = if can_compose {
        crate::build::compose(&world, "world.toml", None)
    } else {
        crate::manifest::load_world(&world, "world.toml").map(|_| ())
    };
    std::fs::remove_dir_all(&scratch).ok();
    result
}

/// A file's bytes before an edit, or its absence.
struct Snapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

impl Snapshot {
    fn take(path: &Path) -> Result<Snapshot, String> {
        let bytes = match std::fs::read(path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("read {}: {e}", path.display())),
        };
        Ok(Snapshot { path: path.to_path_buf(), bytes })
    }

    fn restore(&self) -> Result<(), String> {
        match &self.bytes {
            Some(b) => std::fs::write(&self.path, b),
            None if self.path.exists() => std::fs::remove_file(&self.path),
            None => Ok(()),
        }
        .map_err(|e| format!("restore {}: {e}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_restores_bytes_and_absence() {
        let d = std::env::temp_dir().join(format!("trantor-snap-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (kept, absent) = (d.join("world.toml"), d.join(LOCK_FILE));
        std::fs::write(&kept, "before").unwrap();
        std::fs::remove_file(&absent).ok();
        let (a, b) = (Snapshot::take(&kept).unwrap(), Snapshot::take(&absent).unwrap());
        std::fs::write(&kept, "after").unwrap();
        std::fs::write(&absent, "created").unwrap();
        a.restore().unwrap();
        b.restore().unwrap();
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "before");
        assert!(!absent.exists(), "a file that did not exist before is removed");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_explicit_world_wins_then_a_world_then_a_package() {
        let d = std::env::temp_dir().join(format!("trantor-target-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("package.toml"), "").unwrap();
        assert!(matches!(Target::of(&d, None).unwrap(), Target::Package));
        std::fs::write(d.join("world.toml"), "").unwrap();
        assert!(matches!(Target::of(&d, None).unwrap(), Target::World(w) if w == "world.toml"));
        assert!(matches!(Target::of(&d, Some("alt.toml")).unwrap(), Target::World(w) if w == "alt.toml"));
        std::fs::remove_dir_all(&d).ok();
    }
}
