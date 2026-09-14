//! The scratch worlds `trantor test` and `trantor add` compose a package in.
use std::path::{Path, PathBuf};

use crate::manifest::{Dep, DepSource, Package};

/// A TOML basic string. Rust's `{:?}` is not one: it writes `\u{301}` for a
/// combining accent, which TOML rejects, so a package in a directory named with
/// one failed to compose and blamed the package.
pub fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One `[deps]` line, a path dep resolved against the package root.
fn dep_line(root: &Path, name: &str, dep: &Dep) -> Result<String, String> {
    Ok(match dep.source(name)? {
        DepSource::Path(p) => {
            let abs = root.join(&p);
            let abs = abs.canonicalize().map_err(|e| format!("dependency `{name}` at {}: {e}", abs.display()))?;
            format!("{name} = {{ path = {} }}\n", toml_str(&abs.to_string_lossy()))
        }
        DepSource::GitHub(g) => format!("{name} = {{ github = {} }}\n", toml_str(&g)),
    })
}

/// What a scratch world depends on.
#[derive(Clone, Copy, PartialEq)]
pub enum Deps {
    /// The dev-deps and the package: what consumers compose.
    WithPackage,
    /// Everything the package stands on without the package itself: its
    /// dev-deps AND its own [deps]. A baseline reached through [deps] is still
    /// a baseline, and leaving it out let the negative control and the expect
    /// delta be skipped for any package without dev-deps.
    Baseline,
    /// The dev-deps alone, for a script's world.
    DevOnly,
}

pub fn deps_body(root: &Path, pkg: &Package, which: Deps) -> Result<String, String> {
    let mut names = std::collections::BTreeSet::new();
    let mut body = String::new();
    let mut add = |name: &str, dep: &Dep| -> Result<(), String> {
        if names.insert(name.to_string()) {
            body.push_str(&dep_line(root, name, dep)?);
        }
        Ok(())
    };
    for (name, dep) in &pkg.dev_deps {
        add(name, dep)?;
    }
    if which == Deps::Baseline {
        for (name, dep) in &pkg.deps {
            add(name, dep)?;
        }
    }
    if which == Deps::WithPackage {
        body.push_str(&format!("{} = {{ path = {} }}\n", pkg.package.name, toml_str(&root.to_string_lossy())));
    }
    Ok(body)
}

/// A scratch world at `scratch/<name>`, with the package's trantor.lock copied
/// in (github deps resolve through a world's lock) and `extra_exports` exposed.
pub fn scratch_world(root: &Path, pkg: &Package, scratch: &Path, name: &str, which: Deps, extra_exports: &[String]) -> Result<PathBuf, String> {
    let dir = scratch.join(name);
    std::fs::create_dir_all(dir.join("app")).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let exports = if extra_exports.is_empty() {
        String::new()
    } else {
        format!("exports = [{}]\n", extra_exports.iter().map(|e| toml_str(e)).collect::<Vec<_>>().join(", "))
    };
    let toml = format!("[world]\nname = {}\n{exports}\n[deps]\n{}", toml_str(name), deps_body(root, pkg, which)?);
    std::fs::write(dir.join("world.toml"), toml).map_err(|e| format!("write world.toml: {e}"))?;
    let lock = root.join(crate::registry::LOCK_FILE);
    if lock.is_file() {
        std::fs::copy(&lock, dir.join(crate::registry::LOCK_FILE)).map_err(|e| format!("copy {}: {e}", lock.display()))?;
    }
    Ok(dir)
}

/// Whether a driver is reachable: the package's own, or through its [deps].
pub enum Reach {
    Yes,
    No,
    /// Not knowable without composing, and why.
    Unknown(String),
}

pub fn driver_in_reach(root: &Path, pkg: &Package, depth: usize) -> Reach {
    if pkg.package.provides_driver.is_some() {
        return Reach::Yes;
    }
    deps_reach(root, pkg.deps.iter(), depth)
}

/// Whether what the package stands on — its dev-deps and its [deps], without
/// the package — reaches a driver. A driver package's base does not, and
/// cannot be composed.
pub fn baseline_reach(root: &Path, pkg: &Package) -> Reach {
    deps_reach(root, pkg.dev_deps.iter().chain(pkg.deps.iter()), 0)
}

/// Each dependency's root, parsed.
pub fn dep_packages<'a>(root: &Path, deps: impl Iterator<Item = (&'a String, &'a Dep)>) -> Vec<Result<(String, PathBuf, Package), String>> {
    deps.map(|(name, dep)| {
        let dir = match dep.source(name)? {
            DepSource::Path(p) => root.join(p),
            DepSource::GitHub(g) => crate::registry::github_root(root, name, &g)?,
        };
        let text = std::fs::read_to_string(dir.join("package.toml")).map_err(|e| format!("`{name}`: read {}: {e}", dir.join("package.toml").display()))?;
        let pkg = toml::from_str::<Package>(&text).map_err(|e| format!("`{name}`: parse its package.toml: {e}"))?;
        Ok((name.clone(), dir, pkg))
    })
    .collect()
}

/// Everything the package stands on, transitively: dev-deps, [deps], and
/// their [deps] in turn, each package once.
pub fn all_dep_packages(root: &Path, pkg: &Package) -> Result<Vec<(String, PathBuf, Package)>, String> {
    let mut out: Vec<(String, PathBuf, Package)> = vec![];
    let mut queue = dep_packages(root, pkg.dev_deps.iter().chain(pkg.deps.iter()));
    while let Some(next) = queue.pop() {
        let (name, dir, d) = next?;
        let real = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if out.iter().any(|(_, seen, _)| seen.canonicalize().unwrap_or_else(|_| seen.clone()) == real) {
            continue;
        }
        queue.extend(dep_packages(&dir, d.deps.iter()));
        out.push((name, dir, d));
    }
    Ok(out)
}

fn deps_reach<'a>(root: &Path, deps: impl Iterator<Item = (&'a String, &'a Dep)>, depth: usize) -> Reach {
    if depth > 16 {
        return Reach::Unknown("the [deps] chain is deeper than 16 — a cycle, most likely".into());
    }
    let mut unknown: Option<String> = None;
    for dep in dep_packages(root, deps) {
        match dep.map(|(_, dir, d)| driver_in_reach(&dir, &d, depth + 1)) {
            Ok(Reach::Yes) => return Reach::Yes,
            Ok(Reach::No) => {}
            Ok(Reach::Unknown(why)) | Err(why) => { unknown.get_or_insert(why); }
        }
    }
    unknown.map_or(Reach::No, Reach::Unknown)
}

/// A scratch directory for one run, after removing earlier runs' directories
/// whose process is gone. A failed run keeps its worlds for inspection; nothing
/// used to remove them afterwards, and they reached tens of gigabytes.
pub fn fresh_scratch(name: &str) -> Result<PathBuf, String> {
    fresh_scratch_named("trantor-test", name)
}

/// The same, for another command's scratch directories (`trantor-add-*`).
pub fn fresh_scratch_named(kind: &str, name: &str) -> Result<PathBuf, String> {
    let tmp = std::env::temp_dir();
    let prefix = format!("{kind}-{name}-");
    if let Ok(entries) = std::fs::read_dir(&tmp) {
        for e in entries.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            let Some(pid) = file.strip_prefix(&prefix).and_then(|p| p.parse::<u32>().ok()) else { continue };
            if pid != std::process::id() && !crate::bounded::alive(pid) {
                std::fs::remove_dir_all(e.path()).ok();
            }
        }
    }
    let p = tmp.join(format!("{prefix}{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).map_err(|e| format!("create {}: {e}", p.display()))?;
    Ok(p)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toml_string_escapes_what_toml_requires_and_keeps_unicode() {
        assert_eq!(toml_str("caf\u{65}\u{301} \"q\" \\"), "\"caf\u{65}\u{301} \\\"q\\\" \\\\\"");
        let parsed: toml::Value = toml::from_str(&format!("p = {}", toml_str("a\u{301}\"\\\n"))).unwrap();
        assert_eq!(parsed["p"].as_str(), Some("a\u{301}\"\\\n"));
    }
}
