//! Which Roc modules a package ships, where each one's source is, and which
//! `.roc` files under it hold expects — for `trantor test`'s expects world and
//! its check that no expect is out of reach (T3, T3b).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::manifest::{component_dir, module_path, Package};

pub struct Module {
    pub file: PathBuf,
    pub has_expect: bool,
}

/// Every module the package ships, by the name a platform exposes it under:
/// roc components' exports (a rename `"Path as StrPath"` exposes `StrPath`
/// from `Path.roc`), a driver's contract modules, and each interface's module,
/// event module and env module. Sources are found the way codegen finds them,
/// `roc/<Module>.roc` first. A shim's export that names the interface it
/// fulfils (D19) is not a module.
pub fn shipped(root: &Path, pkg: &Package) -> BTreeMap<String, Module> {
    let mut out = BTreeMap::new();
    let mut add = |exposed: &str, file: PathBuf| {
        let has_expect = has_expect(&file);
        out.insert(exposed.to_string(), Module { file, has_expect });
    };
    for (name, c) in &pkg.components {
        if c.kind != "roc" && c.kind != "driver" {
            continue;
        }
        let dir = component_dir(root, name, c);
        for entry in &c.exports {
            let (source, exposed) = entry.split_once(" as ").map_or((entry.as_str(), entry.as_str()), |(s, e)| (s.trim(), e.trim()));
            if pkg.interfaces.contains_key(source) || pkg.provides.contains_key(source) {
                continue;
            }
            add(exposed, module_path(&dir, source));
        }
    }
    for (iface, r) in &pkg.interfaces {
        let dir = r.dir.clone().unwrap_or_else(|| root.join("interfaces").join(iface));
        let Ok(parsed) = std::fs::read_to_string(dir.join("interface.toml")).map(|t| toml::from_str::<crate::manifest::Interface>(&t)) else { continue };
        let Ok(i) = parsed else { continue };
        for m in std::iter::once(i.module).chain(i.event_module).chain(i.env_module) {
            add(&m, dir.join(format!("{m}.roc")));
        }
    }
    out
}

/// A top-level `expect`: the keyword at the start of a line, then whitespace
/// or the end of the line. `roc test` runs only these; an expect inside a
/// function body runs when the function does, so a module holding only those
/// has nothing for `roc test` to count.
pub fn has_expect(file: &Path) -> bool {
    std::fs::read_to_string(file).is_ok_and(|t| {
        t.lines().any(|l| l.strip_prefix("expect").is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace)))
    })
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// `.roc` files under the package holding a top-level expect, outside the
/// package's own `tests/` and `target/` and any dot-directory. Symlinked
/// directories are followed — a component may live elsewhere and be linked
/// in — and each real directory is walked once, so a link loop ends.
pub fn roc_files_with_expects(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, top: bool, seen: &mut BTreeSet<PathBuf>, out: &mut Vec<PathBuf>) {
        let Ok(real) = dir.canonicalize() else { return };
        if !seen.insert(real) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || (top && (name == "target" || name == "tests")) {
                continue;
            }
            if path.is_dir() {
                walk(&path, false, seen, out);
            } else if path.extension().is_some_and(|x| x == "roc") && has_expect(&path) {
                out.push(path);
            }
        }
    }
    let mut out = vec![];
    walk(root, true, &mut BTreeSet::new(), &mut out);
    out.sort();
    out.dedup_by(|a, b| same_file(a, b));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trantor-modules-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_a_top_level_expect_counts() {
        let d = tmp("expect");
        std::fs::write(d.join("Inline.roc"), "f = |x| {\n    expect x > 0\n    x\n}\n").unwrap();
        std::fs::write(d.join("Top.roc"), "expect\n    1 == 1\n").unwrap();
        assert!(!has_expect(&d.join("Inline.roc")));
        assert!(has_expect(&d.join("Top.roc")));
    }

    #[test]
    fn a_symlinked_component_is_walked_once_and_a_loop_ends() {
        let d = tmp("walk");
        let outside = tmp("outside");
        std::fs::write(outside.join("Helper.roc"), "expect 1 == 2\n").unwrap();
        std::fs::create_dir_all(d.join("components")).unwrap();
        std::os::unix::fs::symlink(&outside, d.join("components/lib")).unwrap();
        std::os::unix::fs::symlink(&d, d.join("components/loop")).unwrap();
        std::fs::create_dir_all(d.join("components/x/tests")).unwrap();
        std::fs::write(d.join("components/x/tests/Deep.roc"), "expect 1 == 1\n").unwrap();
        std::fs::create_dir_all(d.join("tests")).unwrap();
        std::fs::write(d.join("tests/Suite.roc"), "expect 1 == 1\n").unwrap();
        let found: Vec<String> = roc_files_with_expects(&d).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(found, vec!["Helper.roc", "Deep.roc"], "{found:?}");
    }
}
