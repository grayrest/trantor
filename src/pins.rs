//! `trantor update [<name>] [<dir>]` and `trantor remove <name> [<dir>]`: the
//! same argument order as `add`, on a world or a package, and the same rule —
//! the change is kept only if the project still composes.
use std::path::{Path, PathBuf};

use crate::add::{flag_value, is_project};
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
    crate::journal::recover_noting(&dir)?;
    let target = Target::of(&dir, a.world.as_deref())?;
    project::transact(&dir, &target, || {
        crate::registry::update(&dir, target.manifest(), &target.deps(&dir)?, name.as_deref())
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
    crate::journal::recover_noting(&dir)?;
    let target = Target::of(&dir, a.world.as_deref())?;
    project::transact(&dir, &target, || crate::registry::remove(&dir, target.manifest(), &name).map(|()| true))
        .map(|_| eprintln!("trantor: removed `{name}`"))
        .map_err(|e| format!("remove: {e}"))
}

fn names_a_dep(dir: &str, world: Option<&str>, name: &str) -> bool {
    let dir = Path::new(dir);
    Target::of(dir, world).and_then(|t| t.deps(dir)).is_ok_and(|d| d.contains_key(name))
}
