//! `trantor add <org>/<repo> [<dir>]` (D-U1-3, T3b).
//!
//! Adds the dependency to the project in `<dir>` (default: here): its
//! `package.toml`, or its `world.toml`, pinned in that directory's
//! `trantor.lock`. Then it composes, and only a composition that succeeds keeps
//! the edit (see `project::transact`).
use std::path::{Path, PathBuf};

use crate::manifest::DepSource;
use crate::project::{self, Target};
use crate::registry::{self, Lock, LOCK_FILE};

const USAGE: &str = "usage: trantor add <org>/<repo> [<dir>] [--as <name>] [--world <file>]";

pub fn add(dir: &Path, world_flag: Option<&str>, slug: &str, as_name: Option<&str>) -> Result<(), String> {
    let target = Target::of(dir, world_flag)?;
    let place = place(dir, world_flag);
    // Already a dependency: `add` does not move its pin — that is `update`.
    if let Some((present, _)) = target.deps(dir)?.into_iter().find(|(n, d)| matches!(d.source(n), Ok(DepSource::GitHub(s)) if s == slug)) {
        if as_name.is_some_and(|a| a != present) {
            return Err(format!("add {slug}: it is already a dependency in {}, as `{present}`", target.manifest()));
        }
        eprintln!("trantor: `{present}` = {slug} is already in {}; nothing changed. `trantor update {present}{place}` moves its pin", target.manifest());
        return Ok(());
    }
    let fetched = registry::fetch(slug)?;
    let dep_name = as_name.unwrap_or(&fetched.name).to_string();
    name_is_free(dir, &target, &dep_name, slug, &place).map_err(|e| format!("add {slug}: {e}"))?;
    let changed = project::transact(dir, &target, || registry::record(dir, target.manifest(), &dep_name, &fetched))
        .map_err(|e| format!("add {slug}: {e}"))?;
    let pin = fetched.tag.as_deref().map(|t| format!("tag {t}"))
        .unwrap_or_else(|| format!("{} HEAD (no semver tags)", fetched.branch.as_deref().unwrap_or("default branch")));
    eprintln!(
        "trantor: added `{dep_name}` = {slug} to {} at {pin}, commit {}{}",
        target.manifest(),
        &fetched.commit[..fetched.commit.len().min(12)],
        if changed { "" } else { " (already there; nothing changed)" }
    );
    Ok(())
}

/// One name is one package, and one package one name. `add` used to overwrite
/// an existing dep of the same name — a local path checkout included — and to
/// accept a package already present under another name, which composed until
/// the two pins drifted apart. World variants share the directory's lock, so a
/// name pinned for another variant is taken too.
fn name_is_free(dir: &Path, target: &Target, name: &str, slug: &str, place: &str) -> Result<(), String> {
    let deps = target.deps(dir)?;
    if let Some(dep) = deps.get(name) {
        match dep.source(name)? {
            DepSource::GitHub(s) if s == slug => {}
            other => {
                let was = match other { DepSource::GitHub(s) => format!("github {s}"), DepSource::Path(p) => format!("path {p}") };
                return Err(format!(
                    "`{name}` is already a dependency in {} ({was}). Remove it first (`trantor remove {name}{place}`), \
                     or add this one under another name with --as",
                    target.manifest()
                ));
            }
        }
    }
    if let Some(other) = deps.iter().find(|(n, d)| n.as_str() != name && matches!(d.source(n), Ok(DepSource::GitHub(s)) if s == slug)) {
        return Err(format!("{slug} is already a dependency in {}, as `{}`", target.manifest(), other.0));
    }
    if let Some(e) = Lock::load(dir)?.get(name).filter(|e| e.github != slug && !deps.contains_key(name)) {
        return Err(if target.names_used_elsewhere(dir).contains(name) {
            format!(
                "{LOCK_FILE} pins `{name}` to {} for another manifest in this directory. Manifests here share one \
                 lock, so a name means one package — use --as to name this one differently",
                e.github
            )
        } else {
            format!("{LOCK_FILE} still pins `{name}` to {}, though nothing here uses it — `trantor remove {name}{place}` drops the pin", e.github)
        });
    }
    Ok(())
}

/// How to name this project again in a suggested command.
fn place(dir: &Path, world_flag: Option<&str>) -> String {
    let mut s = String::new();
    if dir != Path::new(".") {
        s.push_str(&format!(" {}", dir.display()));
    }
    if let Some(w) = world_flag {
        s.push_str(&format!(" --world {w}"));
    }
    s
}

/// `trantor add <org>/<repo> [<dir>] [--as <name>] [--world <file>]`: the
/// dependency is the first positional, the project directory the optional
/// second (default: the current one).
pub fn command(it: &mut std::iter::Skip<std::slice::Iter<'_, String>>) -> Result<(), String> {
    let (mut positional, mut as_name, mut world): (Vec<String>, Option<String>, Option<String>) = (vec![], None, None);
    while let Some(f) = it.next() {
        match f.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--as" => as_name = Some(dep_name_arg(it.next())?),
            "--world" => world = Some(flag_value(it.next(), "--world", "file")?),
            other if other.starts_with('-') => return Err(format!("unknown flag {other:?}\n{USAGE}")),
            other => positional.push(other.to_string()),
        }
    }
    let (slug, dir) = match positional.as_slice() {
        [slug] => (slug.clone(), PathBuf::from(".")),
        [first, second] if is_project(first) && !is_project(second) => {
            return Err(format!("add: the dependency comes first — try `trantor add {second} {first}`\n{USAGE}"));
        }
        [slug, dir] => (slug.clone(), PathBuf::from(dir)),
        [] => return Err(format!("add: missing <org>/<repo>\n{USAGE}")),
        _ => return Err(format!("add: too many arguments\n{USAGE}")),
    };
    add(&dir, world.as_deref(), &slug, as_name.as_deref())
}

/// A directory holding a project manifest.
pub fn is_project(s: &str) -> bool {
    let p = Path::new(s);
    p.join("world.toml").is_file() || p.join("package.toml").is_file()
}

pub fn flag_value(v: Option<&String>, flag: &str, what: &str) -> Result<String, String> {
    match v {
        Some(v) if !v.is_empty() && !v.starts_with('-') => Ok(v.clone()),
        _ => Err(format!("{flag}: missing {what}")),
    }
}

/// A `[deps]` key: letters, digits, `_` and `-`, not starting with `-`.
fn dep_name_arg(v: Option<&String>) -> Result<String, String> {
    let name = flag_value(v, "--as", "name")?;
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("--as {name:?}: a dependency name is letters, digits, `_` and `-`"));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dependency_name_is_a_plain_toml_key() {
        assert!(dep_name_arg(Some(&"trantor-cli_2".to_string())).is_ok());
        for bad in ["", "--world", "a b", "a.b", "\"q\""] {
            assert!(dep_name_arg(Some(&bad.to_string())).is_err(), "{bad:?} was accepted");
        }
        assert!(dep_name_arg(None).is_err());
    }
}
