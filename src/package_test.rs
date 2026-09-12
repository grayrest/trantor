//! `trantor test <package>`: the checks every package's `verify.sh` used to
//! repeat by hand (T3). Each step drives trantor itself as a subprocess, so a
//! package is tested through exactly the commands a consumer runs.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::manifest::{DepSource, Package};

/// Name of the world an app test composes into. A test app's header points at
/// `../target/trantor/app/platform/main.roc`, so this is part of the contract.
pub const APP_WORLD: &str = "app";

pub fn test_package(dir: &Path) -> Result<(), String> {
    let root = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = std::fs::read_to_string(root.join("package.toml"))
        .map_err(|e| format!("read {}/package.toml: {e}", root.display()))?;
    let pkg: Package = toml::from_str(&text).map_err(|e| format!("parse package.toml: {e}"))?;
    let scratch = Scratch::new(&pkg.package.name)?;
    let result = Steps { root: &root, pkg: &pkg, scratch: &scratch.0 }.all();
    match result {
        Ok(()) => {
            std::fs::remove_dir_all(&scratch.0).ok();
            println!("trantor test: {} PASS", pkg.package.name);
            Ok(())
        }
        Err(e) => Err(format!("{e}\n  (scratch worlds kept at {})", scratch.0.display())),
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Result<Scratch, String> {
        let p = std::env::temp_dir().join(format!("trantor-test-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&p).ok();
        std::fs::create_dir_all(&p).map_err(|e| format!("create {}: {e}", p.display()))?;
        Ok(Scratch(p))
    }
}

pub struct Steps<'a> {
    pub root: &'a Path,
    pub pkg: &'a Package,
    pub scratch: &'a Path,
}

impl Steps<'_> {
    fn all(&self) -> Result<(), String> {
        self.driver()?;
        let with = self.world(APP_WORLD, true)?;
        trantor(&["compose", s(&with)], self.root, "compose the package with its dev-deps")?;
        if !self.pkg.dev_deps.is_empty() {
            println!("ok: composes with its dev-deps");
        }
        let base = if self.is_add_on() && !self.pkg.dev_deps.is_empty() {
            let base = self.world("base", false)?;
            trantor(&["compose", s(&base)], self.root, "compose the dev-deps alone")?;
            self.negative_control(&with, &base)?;
            Some(base)
        } else {
            None
        };
        self.expects(&with, base.as_deref())?;
        crate::readme_examples::check(self, &with)?;
        crate::package_suites::run_all(self, &with)
    }

    pub fn is_add_on(&self) -> bool {
        self.pkg.package.provides_driver.is_none()
    }

    /// `[deps]` for a scratch world: the dev-deps, and the package itself.
    pub fn deps_body(&self, with_package: bool) -> Result<String, String> {
        let mut body = String::new();
        for (name, dep) in &self.pkg.dev_deps {
            let line = match dep.source(name)? {
                DepSource::Path(p) => {
                    let abs = self.root.join(&p);
                    let abs = abs.canonicalize().map_err(|e| format!("dev-dep `{name}` at {}: {e}", abs.display()))?;
                    format!("{name} = {{ path = {:?} }}\n", abs.display().to_string())
                }
                DepSource::GitHub(g) => format!("{name} = {{ github = {g:?} }}\n"),
            };
            body.push_str(&line);
        }
        if with_package {
            body.push_str(&format!("{} = {{ path = {:?} }}\n", self.pkg.package.name, self.root.display().to_string()));
        }
        Ok(body)
    }

    pub fn world(&self, name: &str, with_package: bool) -> Result<PathBuf, String> {
        scratch_world(self.root, self.pkg, self.scratch, name, with_package)
    }

    /// A package with a driver in reach — its own, or one of its `[deps]`'s —
    /// must compose alone. One without must fail, and say it has no driver.
    fn driver(&self) -> Result<(), String> {
        let solo = self.scratch.join("solo");
        std::fs::create_dir_all(solo.join("app")).map_err(|e| format!("create {}: {e}", solo.display()))?;
        let toml = format!("[world]\nname = \"solo\"\n\n[deps]\n{} = {{ path = {:?} }}\n",
            self.pkg.package.name, self.root.display().to_string());
        std::fs::write(solo.join("world.toml"), toml).map_err(|e| format!("write: {e}"))?;
        let out = trantor_output(&["compose", s(&solo)], self.root)?;
        let said = String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);
        match (driver_in_reach(self.root, self.pkg, 0), out.status.success()) {
            (Some(true), true) if self.pkg.package.provides_driver.is_some() => Ok(println!("ok: a baseline, it composes alone on its own driver")),
            (Some(true), true) => Ok(println!("ok: it composes alone, on the driver its dependencies provide")),
            (Some(true), false) => Err(format!("a driver is in reach, but it does not compose alone:\n{said}")),
            (Some(false), true) => Err("no driver is in reach, yet it composed alone".into()),
            (Some(false), false) if !said.contains("names no driver") => Err(format!("alone, it fails for the wrong reason:\n{said}")),
            (Some(false), false) => Ok(println!("ok: alone it says it has no driver")),
            (None, true) => Ok(println!("ok: it composes alone")),
            (None, false) => Err(format!("it does not compose alone, and a github dependency means trantor cannot tell whether it should:\n{said}")),
        }
    }

    /// The dev-deps alone must not provide what this package exports, or every
    /// later check could be passing on the baseline's behalf.
    fn negative_control(&self, with: &Path, base: &Path) -> Result<(), String> {
        let exposed = |w: &Path, name: &str| exposes(&platform_dir(w, name).join("main.roc"));
        let (mine, theirs) = (exposed(with, APP_WORLD)?, exposed(base, "base")?);
        for m in &self.pkg.package.exports {
            if !mine.contains(m) {
                return Err(format!("the package exports {m}, but the composed platform does not expose it"));
            }
            if theirs.contains(m) || platform_dir(base, "base").join(format!("{m}.roc")).exists() {
                return Err(format!("the dev-deps alone already provide {m} — this package is not what supplies it"));
            }
        }
        println!("ok: the dev-deps alone provide none of its {} exported modules", self.pkg.package.exports.len());
        Ok(())
    }

    /// Expects, counted as the package's contribution: the dev-deps bring
    /// their own, so a raw count passes with this package's modules unreachable.
    fn expects(&self, with: &Path, base: Option<&Path>) -> Result<(), String> {
        let both = expect_count(&platform_dir(with, APP_WORLD).join("main.roc"))?;
        let theirs = match base {
            Some(b) => expect_count(&platform_dir(b, "base").join("main.roc"))?,
            None => 0,
        };
        let mine = both.saturating_sub(theirs);
        if mine == 0 && source_has_expects(self.root) {
            return Err(format!("the package's sources contain `expect`s, but it contributed none of the {both} that ran"));
        }
        println!("ok: {both} expects run, {mine} of them this package's own");
        Ok(())
    }
}

/// Whether the package or anything in its `[deps]` provides a driver. `None`
/// when a github dependency is in the chain and cannot be read without
/// fetching it.
pub fn driver_in_reach(root: &Path, pkg: &Package, depth: usize) -> Option<bool> {
    if pkg.package.provides_driver.is_some() {
        return Some(true);
    }
    if depth > 16 {
        return Some(false);
    }
    let mut unknown = false;
    for (name, dep) in &pkg.deps {
        let Ok(DepSource::Path(p)) = dep.source(name) else { unknown = true; continue };
        let dir = root.join(p);
        let parsed = std::fs::read_to_string(dir.join("package.toml")).ok().and_then(|t| toml::from_str::<Package>(&t).ok());
        match parsed.map(|d| driver_in_reach(&dir, &d, depth + 1)) {
            Some(Some(true)) => return Some(true),
            Some(Some(false)) => {}
            _ => unknown = true,
        }
    }
    if unknown { None } else { Some(false) }
}

/// A scratch world at `scratch/<name>` depending on the package's [dev-deps]
/// and, when `with_package`, the package itself — with the package's
/// trantor.lock copied in, since github deps resolve through the world's lock.
pub fn scratch_world(root: &Path, pkg: &Package, scratch: &Path, name: &str, with_package: bool) -> Result<PathBuf, String> {
    let dir = scratch.join(name);
    std::fs::create_dir_all(dir.join("app")).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let steps = Steps { root, pkg, scratch };
    let toml = format!("[world]\nname = \"{name}\"\n\n[deps]\n{}", steps.deps_body(with_package)?);
    std::fs::write(dir.join("world.toml"), toml).map_err(|e| format!("write world.toml: {e}"))?;
    let lock = root.join(crate::registry::LOCK_FILE);
    if lock.is_file() {
        std::fs::copy(&lock, dir.join(crate::registry::LOCK_FILE)).map_err(|e| format!("copy {}: {e}", lock.display()))?;
    }
    Ok(dir)
}

pub fn s(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

pub fn platform_dir(world: &Path, name: &str) -> PathBuf {
    world.join("target/trantor").join(name).join("platform")
}

pub fn trantor_output(args: &[&str], cwd: &Path) -> Result<Output, String> {
    let exe = std::env::current_exe().map_err(|e| format!("locate trantor: {e}"))?;
    Command::new(exe).args(args).current_dir(cwd).output().map_err(|e| format!("spawn trantor: {e}"))
}

pub fn trantor(args: &[&str], cwd: &Path, what: &str) -> Result<Output, String> {
    let out = trantor_output(args, cwd)?;
    if !out.status.success() {
        return Err(format!("{what}:\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    }
    Ok(out)
}

/// The module names a composed platform's `exposes [...]` lists.
fn exposes(main: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(main).map_err(|e| format!("read {}: {e}", main.display()))?;
    let line = text.lines().find(|l| l.trim_start().starts_with("exposes"))
        .ok_or_else(|| format!("{} has no exposes list", main.display()))?;
    let inner = line.split_once('[').and_then(|(_, r)| r.split_once(']')).map(|(i, _)| i).unwrap_or("");
    Ok(inner.split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect())
}

/// `roc test`'s count. "All (0) tests passed" is also what it prints having
/// reached no module at all, which is why the count, not the status, is read.
fn expect_count(main: &Path) -> Result<usize, String> {
    let out = Command::new("perl")
        .args(["-e", "alarm shift; exec @ARGV", "300", &crate::build::roc_bin(), "test", s(main)])
        .output()
        .map_err(|e| format!("spawn roc test: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        return Err(format!("roc test {}:\n{text}", main.display()));
    }
    text.lines()
        .find_map(|l| l.strip_prefix("All (").and_then(|r| r.split_once(')')).and_then(|(n, _)| n.parse().ok()))
        .ok_or_else(|| format!("roc test printed no count:\n{text}"))
}

fn source_has_expects(root: &Path) -> bool {
    fn walk(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        entries.flatten().any(|e| {
            let p = e.path();
            let skip = matches!(p.file_name().and_then(|n| n.to_str()), Some("target" | "tests" | ".git"));
            if p.is_dir() {
                !skip && walk(&p)
            } else {
                p.extension().is_some_and(|x| x == "roc")
                    && std::fs::read_to_string(&p).is_ok_and(|t| t.lines().any(|l| l.trim_start().starts_with("expect ")))
            }
        })
    }
    walk(root)
}
