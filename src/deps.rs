//! `[deps]` expansion (D-U1-1): a dependency is one line, and its own
//! `package.toml` says what it provides, which component implements it, and
//! how it wires by default.
//!
//! Expansion runs BEFORE the world's own entries and never overwrites them, so
//! the explicit `[interfaces]`/`[components]`/`[wiring]` sections stay
//! authoritative. That is not a concession to backwards compatibility: it is
//! what lets a world swap `fs-confined` for `fs-unconfined` or `rusqlite` for
//! `turso`, which is the entire point of a wiring table. Nobody resizing a PNG
//! should have to know it exists; anyone re-wiring a capability must.
//!
//! Dep-vs-dep is a different matter and is an ERROR naming both packages
//! (D-U1-14). Five surfaces collide with no diagnostic otherwise — Roc module
//! files written flat into `platform/`, archive names after `sanitize` maps
//! every non-alphanumeric to `_`, the `[packages]` alias map, one wiring key,
//! and a component name — and "last writer wins" across two strangers' packages
//! is a miscompile, not a preference.

use crate::manifest::{confined, Dep, DepSource, Package, World};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Make a path absolute without requiring it to exist. Dependency paths MUST be
/// absolute by the time they reach `component_dir`, which joins them onto the
/// world dir: a relative one gets joined twice and resolves to
/// `<world>/<world>/pkg/...`, which is a "file not found" naming a path that
/// was never on disk.
fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| {
        std::env::current_dir().map_or_else(|_| p.to_path_buf(), |c| c.join(p))
    })
}

/// How deep a dependency chain may go before we call it a cycle. Deliberately
/// small: a package graph this deep is a mistake long before it is a feature,
/// and the message says which chain hit it.
const MAX_DEPTH: usize = 16;

/// Where a dependency's sources are, and what to say when they are not there.
fn package_root(world_dir: &Path, name: &str, dep: &Dep) -> Result<PathBuf, String> {
    match dep.source(name)? {
        DepSource::Path(p) => {
            let root = world_dir.join(&p);
            if !root.join("package.toml").is_file() {
                return Err(format!(
                    "dep `{name}`: no package.toml at {} — a dependency is a package (its offer), \
                     not a world (a composition).",
                    root.display()
                ));
            }
            Ok(root)
        }
        DepSource::GitHub(slug) => {
            let root = crate::registry::github_root(world_dir, name, &slug)?;
            if !root.join("package.toml").is_file() {
                return Err(format!(
                    "dep `{name}`: {slug} has no package.toml at its root — it is not a trantor \
                     package."
                ));
            }
            Ok(root)
        }
    }
}

/// One package's contribution, staged before it touches the world so that two
/// dependencies colliding is reported as a collision rather than resolved by
/// whichever happened to be expanded second.
#[derive(Default)]
struct Staged {
    interfaces: BTreeMap<String, (PathBuf, String)>, // iface -> (its dir, providing package)
    components: BTreeMap<String, (crate::manifest::Component, String)>,
    wiring: BTreeMap<String, (String, String)>,
    driver: Option<(String, String)>, // (component, package)
    /// Unioned, not claimed: two packages exposing the same module name is a
    /// module collision that `codegen` already refuses when it writes them.
    exports: Vec<String>,
    packages: BTreeMap<String, (String, String)>, // alias -> (url, package)
}

fn claim<T>(
    map: &mut BTreeMap<String, (T, String)>,
    key: &str,
    value: T,
    pkg: &str,
    what: &str,
) -> Result<(), String> {
    if let Some((_, first)) = map.get(key) {
        return Err(format!(
            "two dependencies provide the {what} `{key}`: `{first}` and `{pkg}`. Nothing here can \
             choose between them — drop one, or name the one you want explicitly in world.toml, \
             which overrides both."
        ));
    }
    map.insert(key.to_string(), (value, pkg.to_string()));
    Ok(())
}

fn expand_into(
    world_dir: &Path,
    deps: &BTreeMap<String, Dep>,
    staged: &mut Staged,
    chain: &mut Vec<String>,
    seen: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if chain.len() > MAX_DEPTH {
        return Err(format!(
            "dependency chain deeper than {MAX_DEPTH} — a cycle, most likely: {}",
            chain.join(" -> ")
        ));
    }
    for (name, dep) in deps {
        if chain.iter().any(|c| c == name) {
            return Err(format!(
                "dependency cycle: {} -> {name}",
                chain.join(" -> ")
            ));
        }
        let root = package_root(world_dir, name, dep)?;
        let pkg_file = root.join("package.toml");
        let text = std::fs::read_to_string(&pkg_file)
            .map_err(|e| format!("read {}: {e}", pkg_file.display()))?;
        let pkg: Package = toml::from_str(&text)
            .map_err(|e| format!("parse {}: {e}", pkg_file.display()))?;
        let pkg_name = pkg.package.name.clone();

        // A DIAMOND is not a collision. An app depending on trantor-cli and on
        // trantor-net, which itself depends on trantor-cli, reaches the same
        // package twice — and without this it was reported as "two
        // dependencies provide the component cli-host", naming that package on
        // both sides of its own conflict. Expanded once, keyed by where it
        // actually is.
        // Canonical, not merely absolute: a diamond reaches the same directory
        // by two spellings (`app/../base` and `app/../mid/../base`), and this
        // is an identity test on a directory that certainly exists — we just
        // read its package.toml. (Contrast codegen's path deps, which must be
        // read lexically because they are cargo's to interpret, not ours.)
        let here = std::fs::canonicalize(&root).unwrap_or_else(|_| absolute(&root));
        match seen.get(&pkg_name) {
            Some(first) if *first == here => continue,
            Some(first) => {
                return Err(format!(
                    "package `{pkg_name}` comes from two places: {} and {}. Two copies of one \
                     package cannot be composed together — their components collide by name and \
                     nothing can choose between them.",
                    first.display(),
                    here.display()
                ))
            }
            None => {
                seen.insert(pkg_name.clone(), here);
            }
        }

        // Components: their paths are the PACKAGE's, so rewrite each to an
        // absolute one. `component_dir` joins against the world dir, and an
        // absolute join wins, which is what makes a dependency's crate usable
        // from a world that has never heard of its layout.
        for (cname, mut c) in pkg.components {
            let rel = c.path.clone().unwrap_or_else(|| format!("components/{cname}"));
            let abs = absolute(&confined(&root, &rel, "a component path", &pkg_name)?);
            c.path = Some(abs.to_string_lossy().into_owned());
            c.pkg_root = Some(absolute(&root).to_string_lossy().into_owned());
            claim(&mut staged.components, &cname, c, &pkg_name, "component")?;
        }
        for (iname, _) in pkg.interfaces {
            let idir = absolute(&root.join("interfaces").join(&iname));
            claim(&mut staged.interfaces, &iname, idir, &pkg_name, "interface")?;
        }
        for (iface, expr) in pkg.provides {
            claim(&mut staged.wiring, &iface, expr, &pkg_name, "wiring for")?;
        }
        for e in pkg.package.exports {
            if !staged.exports.contains(&e) {
                staged.exports.push(e);
            }
        }
        for (alias, url) in pkg.packages {
            if let Some((first_url, first_pkg)) = staged.packages.get(&alias) {
                if *first_url != url {
                    return Err(format!(
                        "two dependencies bind the Roc package alias `{alias}` to different \
                         URLs: `{first_pkg}` and `{pkg_name}`. An alias is a single import \
                         name; rename one, or pin both to the same release."
                    ));
                }
            }
            staged.packages.insert(alias, (url, pkg_name.clone()));
        }
        if let Some(d) = pkg.package.provides_driver {
            if let Some((first, first_pkg)) = &staged.driver {
                return Err(format!(
                    "two dependencies provide a driver: `{first}` from `{first_pkg}` and `{d}` \
                     from `{pkg_name}`. A world has exactly one runtime provider — name the one \
                     you want with `[world] driver`."
                ));
            }
            staged.driver = Some((d, pkg_name.clone()));
        }

        chain.push(name.clone());
        expand_into(&root, &pkg.deps, staged, chain, seen)?;
        chain.pop();
    }
    Ok(())
}

/// Expand `[deps]` into the world. Called by `load_world` before anything reads
/// the component or wiring maps.
pub fn expand(world_dir: &Path, world: &mut World) -> Result<(), String> {
    if world.deps.is_empty() {
        return Ok(());
    }
    let mut staged = Staged::default();
    let mut chain = Vec::new();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    let deps = std::mem::take(&mut world.deps);
    let result = expand_into(world_dir, &deps, &mut staged, &mut chain, &mut seen);
    world.deps = deps;
    result?;

    // The world's own entries win, by name and silently — that IS the override.
    for (name, (c, _)) in staged.components {
        world.components.entry(name).or_insert(c);
    }
    for (name, (dir, _)) in staged.interfaces {
        world
            .interfaces
            .entry(name)
            .or_default()
            .dir
            .get_or_insert(dir);
    }
    for (iface, (expr, _)) in staged.wiring {
        world.wiring.entry(iface).or_insert(expr);
    }
    for e in staged.exports {
        if !world.world.exports.contains(&e) {
            world.world.exports.push(e);
        }
    }
    for (alias, (url, _)) in staged.packages {
        world.packages.entry(alias).or_insert(url);
    }
    if world.world.driver.is_none() {
        if let Some((d, _)) = staged.driver {
            world.world.driver = Some(d);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::load_world;

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// A package offering one interface, one component and a default wiring.
    fn pkg(root: &Path, name: &str, iface: &str, comp: &str, driver: Option<&str>) {
        let drv = driver.map_or(String::new(), |d| format!("provides_driver = \"{d}\"\n"));
        write(
            root,
            "package.toml",
            &format!(
                "[package]\nname = \"{name}\"\n{drv}\n[provides]\n{iface} = \"{comp}\"\n\
                 \n[interfaces.{iface}]\n\n[components.{comp}]\nkind = \"host\"\n"
            ),
        );
        write(root, &format!("interfaces/{iface}/interface.toml"), "module = \"M\"\n");
    }

    /// A unique scratch directory. Keyed by an atomic counter, NOT by the
    /// clock: macOS `SystemTime` granularity lets two parallel tests land on
    /// the same nanosecond, and then one test's cleanup deletes the other's
    /// fixture. That was flaky 2 runs in 5 before the counter.
    fn tmp() -> PathBuf {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "trantor-deps-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn a_path_dep_supplies_components_wiring_and_the_driver() {
        let t = tmp();
        pkg(&t.join("base"), "base", "text", "text-host", Some("drv"));
        write(&t.join("app"), "world.toml", "[world]\nname = \"a\"\n\n[deps]\nbase = { path = \"../base\" }\n");
        let w = load_world(&t.join("app"), "world.toml").unwrap();
        assert_eq!(w.driver().unwrap(), "drv", "driver inherited from the dep (D-U1-6)");
        assert!(w.components.contains_key("text-host"));
        assert_eq!(w.wiring.get("text").unwrap(), "text-host");
        // The component's path was rewritten to the package's, absolutely.
        let p = w.components["text-host"].path.clone().unwrap();
        assert!(Path::new(&p).is_absolute() && p.contains("base"), "got {p}");
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn a_packages_dev_deps_never_reach_its_consumer() {
        let t = tmp();
        pkg(&t.join("base"), "base", "text", "text-host", Some("drv"));
        // A dev-dep that does not even exist: if a consumer expanded it, the
        // load would fail reading its package.toml.
        let mut body = std::fs::read_to_string(t.join("base/package.toml")).unwrap();
        body.push_str("\n[dev-deps]\nharness = { path = \"../nowhere\" }\n");
        write(&t.join("base"), "package.toml", &body);
        write(&t.join("app"), "world.toml", "[world]\nname = \"a\"\n\n[deps]\nbase = { path = \"../base\" }\n");
        let w = load_world(&t.join("app"), "world.toml").expect("dev-deps must not be expanded for a consumer");
        assert!(w.components.contains_key("text-host"));
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn the_world_overrides_a_deps_wiring_by_name() {
        let t = tmp();
        pkg(&t.join("base"), "base", "fs", "fs-unconfined", None);
        write(
            &t.join("app"),
            "world.toml",
            "[world]\nname = \"a\"\ndriver = \"d\"\n\n[deps]\nbase = { path = \"../base\" }\n\
             \n[wiring]\nfs = \"fs-confined\"\n",
        );
        let w = load_world(&t.join("app"), "world.toml").unwrap();
        assert_eq!(
            w.wiring.get("fs").unwrap(),
            "fs-confined",
            "the world's own wiring must beat the dependency's default"
        );
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn a_diamond_expands_the_shared_package_once() {
        // app -> base, and app -> mid -> base. The same package reached twice
        // is not two packages.
        let t = tmp();
        pkg(&t.join("base"), "base", "text", "text-host", Some("drv"));
        write(
            &t.join("mid"),
            "package.toml",
            "[package]\nname = \"mid\"\n\n[deps]\nbase = { path = \"../base\" }\n",
        );
        write(
            &t.join("app"),
            "world.toml",
            "[world]\nname = \"a\"\n\n[deps]\nbase = { path = \"../base\" }\nmid = { path = \"../mid\" }\n",
        );
        let w = load_world(&t.join("app"), "world.toml").unwrap();
        assert_eq!(w.driver().unwrap(), "drv");
        assert!(w.components.contains_key("text-host"));
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn one_package_from_two_places_is_an_error_naming_both() {
        let t = tmp();
        pkg(&t.join("one"), "base", "a", "ca", None);
        pkg(&t.join("two"), "base", "b", "cb", None);   // same package NAME
        write(
            &t.join("app"),
            "world.toml",
            "[world]\nname = \"a\"\ndriver = \"d\"\n\n[deps]\nx = { path = \"../one\" }\ny = { path = \"../two\" }\n",
        );
        let e = load_world(&t.join("app"), "world.toml").unwrap_err();
        assert!(e.contains("two places"), "{e}");
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn two_deps_claiming_one_wiring_key_is_an_error_naming_both() {
        let t = tmp();
        pkg(&t.join("one"), "one", "fs", "fs-a", None);
        pkg(&t.join("two"), "two", "fs", "fs-b", None);
        write(
            &t.join("app"),
            "world.toml",
            "[world]\nname = \"a\"\ndriver = \"d\"\n\n[deps]\none = { path = \"../one\" }\n\
             two = { path = \"../two\" }\n",
        );
        let e = load_world(&t.join("app"), "world.toml").unwrap_err();
        assert!(e.contains("`one`") && e.contains("`two`"), "must name both packages: {e}");
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn two_deps_providing_a_driver_is_an_error() {
        let t = tmp();
        pkg(&t.join("one"), "one", "a", "ca", Some("d1"));
        pkg(&t.join("two"), "two", "b", "cb", Some("d2"));
        write(
            &t.join("app"),
            "world.toml",
            "[world]\nname = \"a\"\n\n[deps]\none = { path = \"../one\" }\ntwo = { path = \"../two\" }\n",
        );
        let e = load_world(&t.join("app"), "world.toml").unwrap_err();
        assert!(e.contains("d1") && e.contains("d2"), "{e}");
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn a_dep_path_escaping_its_package_is_refused() {
        let t = tmp();
        let base = t.join("base");
        pkg(&base, "base", "x", "cx", None);
        // The component points outside the package — the D-U1-13 case.
        write(
            &base,
            "package.toml",
            "[package]\nname = \"base\"\n\n[provides]\nx = \"cx\"\n\n[components.cx]\n\
             kind = \"host\"\npath = \"../../../etc\"\n",
        );
        write(&t.join("app"), "world.toml", "[world]\nname = \"a\"\ndriver = \"d\"\n\n[deps]\nbase = { path = \"../base\" }\n");
        let e = load_world(&t.join("app"), "world.toml").unwrap_err();
        assert!(e.contains("escapes the package root"), "{e}");
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn a_github_dep_says_how_to_fetch_it() {
        let t = tmp();
        write(&t.join("app"), "world.toml", "[world]\nname = \"a\"\ndriver = \"d\"\n\n[deps]\nb = { github = \"org/repo\" }\n");
        let e = load_world(&t.join("app"), "world.toml").unwrap_err();
        assert!(e.contains("trantor add org/repo"), "{e}");
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn a_dependency_cycle_is_reported_not_hung() {
        let t = tmp();
        pkg(&t.join("one"), "one", "a", "ca", None);
        write(&t.join("one"), "package.toml",
            "[package]\nname = \"one\"\n\n[provides]\na = \"ca\"\n\n[components.ca]\nkind = \"host\"\n\
             \n[deps]\ntwo = { path = \"../two\" }\n");
        pkg(&t.join("two"), "two", "b", "cb", None);
        write(&t.join("two"), "package.toml",
            "[package]\nname = \"two\"\n\n[provides]\nb = \"cb\"\n\n[components.cb]\nkind = \"host\"\n\
             \n[deps]\none = { path = \"../one\" }\n");
        write(&t.join("app"), "world.toml", "[world]\nname = \"a\"\ndriver = \"d\"\n\n[deps]\none = { path = \"../one\" }\n");
        let e = load_world(&t.join("app"), "world.toml").unwrap_err();
        assert!(e.contains("cycle"), "{e}");
        std::fs::remove_dir_all(&t).ok();
    }
}
