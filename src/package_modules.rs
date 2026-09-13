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

/// A top-level `expect`: the keyword outside any bracket, followed by
/// whitespace, `(` or the end of the line — however it is indented, since Roc
/// runs `  expect x` and `expect(x)` at module level too. `roc test` runs only
/// these; an expect inside a function body runs when the function does, so a
/// module holding only those has nothing for `roc test` to count.
pub fn has_expect(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else { return false };
    // Module level is depth 0, and depth 1 inside a type module's `.{ ... }`
    // body, which is where a module's expects usually live — `roc test` runs
    // those too.
    let (mut depth, mut in_type_body) = (0, false);
    for line in text.lines() {
        let module_level = depth <= 0 || (in_type_body && depth == 1);
        let rest = line.trim_start().strip_prefix("expect");
        if module_level && rest.is_some_and(|r| r.is_empty() || r.starts_with(|c: char| c.is_whitespace() || c == '(')) {
            return true;
        }
        let opens_type_body = depth <= 0 && starts_type_module(line);
        depth += crate::readme_lex::scan(line).depth;
        if opens_type_body && depth == 1 {
            in_type_body = true;
        } else if depth <= 0 {
            in_type_body = false;
        }
    }
    false
}

/// `Name :: ... .{` or `Name := ... .{`: a type module's opening line.
fn starts_type_module(line: &str) -> bool {
    line.split_once(" :").is_some_and(|(name, rest)| {
        name.starts_with(|c: char| c.is_ascii_uppercase()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && (rest.starts_with(':') || rest.starts_with('=')) && rest.trim_end().ends_with(".{")
    })
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// `.roc` files holding a top-level expect: under the package, outside its own
/// `tests/` and `target/` and any dot-directory, and under every directory its
/// manifest names — a component at `.vendor/lib` or `tests/support/lib` is
/// still its code. Symlinked directories are followed, and each real directory
/// is walked once, so a link loop ends.
pub fn roc_files_with_expects(root: &Path, pkg: &Package) -> Vec<PathBuf> {
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
    let mut seen = BTreeSet::new();
    walk(root, true, &mut seen, &mut out);
    let named = pkg.components.iter().map(|(name, c)| component_dir(root, name, c))
        .chain(pkg.interfaces.iter().map(|(i, r)| r.dir.clone().unwrap_or_else(|| root.join("interfaces").join(i))));
    for dir in named {
        // Walked from scratch: a directory skipped above is not in `seen`.
        walk(&dir, false, &mut seen, &mut out);
    }
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
    fn an_indented_or_parenthesised_module_level_expect_counts() {
        let d = tmp("forms");
        std::fs::write(d.join("Paren.roc"), "x = 1\nexpect(x == 2)\n").unwrap();
        std::fs::write(d.join("Indented.roc"), "x = 1\n  expect x == 2\n").unwrap();
        std::fs::write(d.join("Body.roc"), "f = |x| {\n  expect(x > 0)\n  x\n}\n").unwrap();
        assert!(has_expect(&d.join("Paren.roc")) && has_expect(&d.join("Indented.roc")));
        assert!(!has_expect(&d.join("Body.roc")));
        std::fs::write(d.join("TypeBody.roc"), "Helper :: [].{\n\tx : I64\n\tx = 1\n\texpect x == 2\n}\n").unwrap();
        std::fs::write(d.join("MethodBody.roc"), "Helper :: [].{\n\tf = |x| {\n\t\texpect x > 0\n\t\tx\n\t}\n}\n").unwrap();
        std::fs::write(d.join("MidString.roc"), "x = \\\\usage (see below\nexpect x == \"nope\"\n").unwrap();
        assert!(has_expect(&d.join("TypeBody.roc")), "an expect in a type module's body");
        assert!(!has_expect(&d.join("MethodBody.roc")), "an expect in a method's body");
        assert!(has_expect(&d.join("MidString.roc")), "brackets in a mid-line multi-line string");
    }

    #[test]
    fn a_component_under_a_skipped_directory_is_still_walked() {
        let d = tmp("vendored");
        std::fs::create_dir_all(d.join(".vendor/lib")).unwrap();
        std::fs::write(d.join(".vendor/lib/Helper.roc"), "expect 1 == 2\n").unwrap();
        let pkg: Package = toml::from_str("[package]\nname = \"p\"\n\n[components.lib]\nkind = \"roc\"\npath = \".vendor/lib\"\n").unwrap();
        assert_eq!(roc_files_with_expects(&d, &pkg).len(), 1);
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
        let found: Vec<String> = roc_files_with_expects(&d, &toml::from_str("[package]\nname = \"p\"\n").unwrap()).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(found, vec!["Helper.roc", "Deep.roc"], "{found:?}");
    }
}
