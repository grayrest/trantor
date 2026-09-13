//! `trantor update [<name>] [<dir>]` and `trantor remove <name> [<dir>]`: the
//! same argument order as `add`, on a world or a package, and the same rule —
//! the change is kept only if the project still composes.
use std::path::{Path, PathBuf};

use crate::add::{flag_value, is_project};
use crate::manifest::{Dep, DepSource};
use crate::registry::{edit_world, ensure_cached, resolve, Entry, Lock, Resolved};
use crate::project::{self, Target};

const UPDATE_USAGE: &str = "usage: trantor update [<name>] [<dir>] [--world <file>]   (every pin: trantor update --all [<dir>])";
const REMOVE_USAGE: &str = "usage: trantor remove <name> [<dir>] [--world <file>]";

struct Args {
    positional: Vec<String>,
    world: Option<String>,
    all: bool,
}

fn parse(it: &mut std::iter::Skip<std::slice::Iter<'_, String>>, usage: &str) -> Result<Option<Args>, String> {
    let mut a = Args { positional: vec![], world: None, all: false };
    while let Some(f) = it.next() {
        match f.as_str() {
            "-h" | "--help" => {
                println!("{usage}");
                return Ok(None);
            }
            "--world" => a.world = Some(flag_value(it.next(), "--world", "file")?),
            "--all" => a.all = true,
            other if other.starts_with('-') => return Err(format!("unknown flag {other:?}\n{usage}")),
            other => a.positional.push(other.to_string()),
        }
    }
    Ok(Some(a))
}

pub fn update_command(it: &mut std::iter::Skip<std::slice::Iter<'_, String>>) -> Result<(), String> {
    let Some(a) = parse(it, UPDATE_USAGE)? else { return Ok(()) };
    let (name, dir) = match (a.all, a.positional.as_slice()) {
        (true, []) => (None, PathBuf::from(".")),
        (true, [dir]) => (None, PathBuf::from(dir)),
        (true, _) => return Err(format!("update --all takes no <name>\n{UPDATE_USAGE}")),
        (false, []) => (None, PathBuf::from(".")),
        (false, [one]) if is_project(one) && !names_a_dep(".", a.world.as_deref(), one) => {
            return Err(format!("update: `{one}` is a project directory, not a dependency here — `trantor update --all {one}` moves every pin in it\n{UPDATE_USAGE}"));
        }
        (false, [name]) => (Some(name.clone()), PathBuf::from(".")),
        (false, [first, second]) if is_project(first) && !is_project(second) => {
            return Err(format!("update: the name comes first — try `trantor update {second} {first}`\n{UPDATE_USAGE}"));
        }
        (false, [name, dir]) => (Some(name.clone()), PathBuf::from(dir)),
        _ => return Err(format!("update: too many arguments\n{UPDATE_USAGE}")),
    };
    crate::journal::recover_for_edit(&dir)?;
    let target = Target::of(&dir, a.world.as_deref())?;
    project::transact(&dir, &target, || {
        update(&dir, target.manifest(), &target.deps(&dir)?, name.as_deref())
    })
    .map(|_| ())
    .map_err(|e| format!("update: {e}"))
}

pub fn remove_command(it: &mut std::iter::Skip<std::slice::Iter<'_, String>>) -> Result<(), String> {
    let Some(a) = parse(it, REMOVE_USAGE)? else { return Ok(()) };
    if a.all {
        return Err(format!("unknown flag \"--all\"\n{REMOVE_USAGE}"));
    }
    let (name, dir) = match a.positional.as_slice() {
        [first, second] if is_project(first) && !is_project(second) => {
            return Err(format!("remove: the name comes first — try `trantor remove {second} {first}`\n{REMOVE_USAGE}"));
        }
        [name] => (name.clone(), PathBuf::from(".")),
        [name, dir] => (name.clone(), PathBuf::from(dir)),
        [] => return Err(format!("remove: missing <name>\n{REMOVE_USAGE}")),
        _ => return Err(format!("remove: too many arguments\n{REMOVE_USAGE}")),
    };
    crate::journal::recover_for_edit(&dir)?;
    let target = Target::of(&dir, a.world.as_deref())?;
    let elsewhere = target.names_used_elsewhere(&dir).contains(&name);
    let mut said = String::new();
    project::transact(&dir, &target, || {
        said = remove(&dir, target.manifest(), &name, elsewhere)?;
        Ok(true)
    })
    .map(|_| eprintln!("trantor: {said}"))
    .map_err(|e| format!("remove: {e}"))
}

fn names_a_dep(dir: &str, world: Option<&str>, name: &str) -> bool {
    let dir = Path::new(dir);
    Target::of(dir, world).and_then(|t| t.deps(dir)).is_ok_and(|d| d.contains_key(name))
}

/// `trantor update [<name>]` — move a pin. Without this the lock's own header
/// tells the user to edit a file that cannot express a version.
pub fn update(dir: &Path, manifest: &str, deps: &std::collections::BTreeMap<String, Dep>, only: Option<&str>) -> Result<bool, String> {
    let mut lock = Lock::load(dir)?;
    let mut moved = 0;
    let mut seen = 0;
    for (name, dep) in deps {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let DepSource::GitHub(slug) = dep.source(name)? else { continue };
        seen += 1;
        let resolved = resolve(&slug)?;
        let (tag, branch, commit) = match resolved {
            Resolved::Tag(t, c) => (Some(t), None, c),
            Resolved::Branch(b, c) => (None, Some(b), c),
        };
        let before = lock.get(name).map(|e| e.commit.clone());
        // The same commit under another repo — a fork — is still a move: the
        // pin must name the repo the manifest does.
        if lock.get(name).is_some_and(|e| e.commit == commit && e.github == slug) {
            eprintln!("trantor: `{name}` is already at {}", &commit[..12.min(commit.len())]);
            continue;
        }
        ensure_cached(&slug, &commit)?;
        lock.packages.retain(|e| e.name != *name);
        lock.packages.push(Entry { name: name.clone(), github: slug, tag: tag.clone(), branch, commit: commit.clone() });
        moved += 1;
        eprintln!(
            "trantor: `{name}` {} -> {}{}",
            before.as_deref().map_or("(unpinned)".into(), |c| c[..12.min(c.len())].to_string()),
            &commit[..12.min(commit.len())],
            tag.map_or(String::new(), |t| format!(" ({t})"))
        );
    }
    if let Some(o) = only {
        if seen == 0 {
            return Err(format!("no github dep named `{o}` in {manifest}"));
        }
    } else if seen == 0 {
        return Err(format!("{manifest} has no github deps to update"));
    }
    let changed = lock.save(dir)?;
    eprintln!("trantor: {moved} of {seen} dep(s) moved");
    Ok(changed)
}

/// `trantor remove <name>` — drop the dep from `[deps]` or `[dev-deps]`, and
/// its pin unless another manifest in the directory still uses it. A pin left
/// behind with no manifest line is dropped on its own. What it did, for the
/// user.
pub fn remove(dir: &Path, manifest: &str, name: &str, used_elsewhere: bool) -> Result<String, String> {
    let (p, mut doc) = edit_world(dir, manifest)?;
    let mut had = false;
    for table in ["deps", "dev-deps"] {
        if doc.get_mut(table).and_then(|d| d.as_table_like_mut()).is_some_and(|t| t.remove(name).is_some()) {
            had = true;
            if doc.get(table).and_then(|d| d.as_table_like()).is_some_and(|t| t.is_empty()) {
                doc.remove(table);
            }
        }
    }
    let mut lock = Lock::load(dir)?;
    let pinned = lock.get(name).is_some();
    if !had && (!pinned || used_elsewhere) {
        return Err(format!("{manifest} has no dep named `{name}`"));
    }
    if had {
        crate::journal::write_atomic(&p, doc.to_string().as_bytes())?;
    }
    if used_elsewhere {
        return Ok(format!("removed `{name}` from {manifest}; its pin stays, another manifest here uses it"));
    }
    lock.packages.retain(|e| e.name != name);
    lock.save(dir)?;
    Ok(if had { format!("removed `{name}`") } else { format!("dropped the leftover pin for `{name}`") })
}
