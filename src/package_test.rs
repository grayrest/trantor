//! `trantor test <package>`: the checks every package's `verify.sh` used to
//! repeat by hand (T3). Each step drives trantor itself as a subprocess, so a
//! package is tested through exactly the commands a consumer runs.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::bounded::{self, Ran};
use crate::manifest::Package;
use crate::package_modules::{roc_files_with_expects, same_file, Module};
use crate::package_worlds::{driver_in_reach, scratch_world, Deps, Reach};

/// The world app suites compose into. A test app's header
/// points at `../target/trantor/app/platform/main.roc`, so this is part of the
/// contract. It exposes exactly what consumers see.
pub const APP_WORLD: &str = "app";
/// The world expects run in: the app world plus every module the package
/// ships, so an expect in a module it keeps unexported still runs.
const EXPECTS_WORLD: &str = "expects";
const BASE_WORLD: &str = "base";

pub fn test_package(dir: &Path) -> Result<(), String> {
    let root = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = std::fs::read_to_string(root.join("package.toml"))
        .map_err(|e| format!("read {}/package.toml: {e}", root.display()))?;
    let pkg: Package = toml::from_str(&text).map_err(|e| format!("parse package.toml: {e}"))?;
    let scratch = crate::package_worlds::fresh_scratch(&pkg.package.name)?;
    match (Steps { root: &root, pkg: &pkg, scratch: &scratch }).all() {
        Ok(()) => {
            std::fs::remove_dir_all(&scratch).ok();
            println!("trantor test: {} PASS", pkg.package.name);
            Ok(())
        }
        Err(e) => Err(format!("{e}\n  (scratch worlds kept at {})", scratch.display())),
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
        let with = self.world(APP_WORLD, Deps::WithPackage, &[])?;
        self.trantor(&["compose", s(&with)], "compose the package with its dev-deps")?;
        if !self.pkg.dev_deps.is_empty() {
            println!("ok: composes with its dev-deps");
        }
        let modules = crate::package_modules::shipped(self.root, self.pkg);
        let shipped: Vec<String> = modules.keys().cloned().collect();
        let expects = self.world(EXPECTS_WORLD, Deps::WithPackage, &shipped)?;
        self.trantor(&["compose", s(&expects)], "compose the package with every module it ships exposed")?;
        let stands_on_nothing = crate::package_worlds::deps_body(self.root, self.pkg, Deps::Baseline)?.is_empty();
        let base = match crate::package_worlds::baseline_reach(self.root, self.pkg) {
            _ if stands_on_nothing => None,
            // A driver package's dependencies have no driver without it, so
            // they cannot compose alone; what they export is read instead.
            Reach::No => {
                self.negative_control_uncomposed(&with)?;
                None
            }
            Reach::Yes | Reach::Unknown(_) => {
                let base = self.world(BASE_WORLD, Deps::Baseline, &[])?;
                self.trantor(&["compose", s(&base)], "compose what the package stands on, without it")?;
                self.negative_control(&with, &base)?;
                Some(base)
            }
        };
        self.expects(&expects, base.as_deref(), &modules)?;
        crate::package_suites::run_all(self, &with)
    }

    pub fn world(&self, name: &str, which: Deps, extra: &[String]) -> Result<PathBuf, String> {
        scratch_world(self.root, self.pkg, self.scratch, name, which, extra)
    }

    /// trantor as a subprocess, bounded; success required.
    pub fn trantor(&self, args: &[&str], what: &str) -> Result<Ran, String> {
        let ran = self.trantor_ran(args)?;
        if !ran.ok() {
            return Err(ran.failure(what));
        }
        Ok(ran)
    }

    pub fn trantor_ran(&self, args: &[&str]) -> Result<Ran, String> {
        let exe = std::env::current_exe().map_err(|e| format!("locate trantor: {e}"))?;
        bounded::run(Command::new(exe).args(args).current_dir(self.root), &format!("trantor {}", args.join(" ")), &self.scratch.join("runs"))
    }

    /// A package with a driver in reach — its own, or one of its `[deps]`'s —
    /// must compose alone. One without must fail, and say it has no driver.
    fn driver(&self) -> Result<(), String> {
        let solo = self.scratch.join("solo");
        std::fs::create_dir_all(solo.join("app")).map_err(|e| format!("create {}: {e}", solo.display()))?;
        let toml = format!("[world]\nname = \"solo\"\n\n[deps]\n{} = {{ path = {} }}\n",
            self.pkg.package.name, crate::package_worlds::toml_str(&self.root.to_string_lossy()));
        std::fs::write(solo.join("world.toml"), toml).map_err(|e| format!("write: {e}"))?;
        let ran = self.trantor_ran(&["compose", s(&solo)])?;
        let said = format!("{}{}", ran.stdout, ran.stderr);
        let own = self.pkg.package.provides_driver.is_some();
        match (driver_in_reach(self.root, self.pkg, 0), ran.ok()) {
            (Reach::Yes, true) if own => Ok(println!("ok: a baseline, it composes alone on its own driver")),
            (Reach::Yes, true) => Ok(println!("ok: it composes alone, on the driver its dependencies provide")),
            (Reach::Yes, false) => Err(format!("a driver is in reach, but it does not compose alone:\n{said}")),
            (Reach::No, true) => Err("no driver is in reach, yet it composed alone".into()),
            (Reach::No, false) if !said.contains("names no driver") => Err(format!("alone, it fails for the wrong reason:\n{said}")),
            (Reach::No, false) => Ok(println!("ok: alone it says it has no driver")),
            (Reach::Unknown(_), true) => Ok(println!("ok: it composes alone")),
            (Reach::Unknown(why), false) => Err(format!("it does not compose alone, and whether it should is unknown ({why}):\n{said}")),
        }
    }

    /// What the package stands on must not already provide what it exports, or
    /// every later check could be passing on that baseline's behalf.
    fn negative_control(&self, with: &Path, base: &Path) -> Result<(), String> {
        let exposed = |w: &Path, name: &str| exposes(&platform_dir(w, name).join("main.roc"));
        let (mine, theirs) = (exposed(with, APP_WORLD)?, exposed(base, BASE_WORLD)?);
        for m in &self.pkg.package.exports {
            if !mine.contains(m) {
                return Err(format!("the package exports {m}, but the composed platform does not expose it"));
            }
            if theirs.contains(m) || platform_dir(base, BASE_WORLD).join(format!("{m}.roc")).exists() {
                return Err(format!("what it stands on already provides {m} — this package is not what supplies it"));
            }
        }
        println!("ok: what it stands on provides none of its {} exported modules", self.pkg.package.exports.len());
        Ok(())
    }

    /// The negative control for dependencies that cannot compose without the
    /// package: none of the modules they ship is one it exports.
    fn negative_control_uncomposed(&self, with: &Path) -> Result<(), String> {
        let mine = exposes(&platform_dir(with, APP_WORLD).join("main.roc"))?;
        let deps = crate::package_worlds::all_dep_packages(self.root, self.pkg)?;
        for m in &self.pkg.package.exports {
            if !mine.contains(m) {
                return Err(format!("the package exports {m}, but the composed platform does not expose it"));
            }
            for (name, dir, d) in &deps {
                if crate::package_modules::shipped(dir, d).contains_key(m) {
                    return Err(format!("its dependency `{name}` already ships {m} — this package is not what supplies it"));
                }
            }
        }
        println!("ok: what it stands on ships none of its {} exported modules (read from their manifests: without this driver they cannot compose)", self.pkg.package.exports.len());
        Ok(())
    }

    /// The package's expects run and are counted as its contribution. Every
    /// `.roc` file holding an expect must belong to a module the package ships,
    /// or no world can reach it: an unexported helper's failing expect used to
    /// pass silently.
    fn expects(&self, expects: &Path, base: Option<&Path>, modules: &BTreeMap<String, Module>) -> Result<(), String> {
        let exposed = exposes(&platform_dir(expects, EXPECTS_WORLD).join("main.roc"))?;
        for file in roc_files_with_expects(self.root, self.pkg) {
            let shipped = modules.iter().any(|(name, m)| same_file(&m.file, &file) && exposed.contains(name));
            if !shipped {
                let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                return Err(format!(
                    "{} holds expects, but {stem} is not a module any component exports or interface declares, so nothing can run them",
                    file.strip_prefix(self.root).unwrap_or(&file).display()
                ));
            }
        }
        let both = self.expect_count(&platform_dir(expects, EXPECTS_WORLD).join("main.roc"))?;
        let theirs = match base {
            Some(b) => self.expect_count(&platform_dir(b, BASE_WORLD).join("main.roc"))?,
            None => 0,
        };
        let mine = both.saturating_sub(theirs);
        let with_expects = modules.values().filter(|m| m.has_expect).count();
        if with_expects > 0 && mine == 0 {
            return Err(format!("{with_expects} of its modules hold expects, but none of the {both} that ran were its own"));
        }
        println!("ok: {both} expects run, {mine} of them this package's own (every module it ships exposed)");
        Ok(())
    }

    /// `roc test`'s count. "All (0) tests passed" is also what it prints having
    /// reached no module at all, which is why the count, not the status, is read.
    fn expect_count(&self, main: &Path) -> Result<usize, String> {
        let ran = bounded::run(Command::new(crate::build::roc_bin()).args(["test", s(main)]), "roc test", &self.scratch.join("runs"))?;
        let text = format!("{}{}", ran.stdout, ran.stderr);
        if !ran.ok() {
            return Err(ran.failure(&format!("roc test {}", main.display())));
        }
        text.lines()
            .find_map(|l| l.strip_prefix("All (").and_then(|r| r.split_once(')')).and_then(|(n, _)| n.parse().ok()))
            .ok_or_else(|| format!("roc test printed no count:\n{text}"))
    }
}

pub fn s(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

pub fn platform_dir(world: &Path, name: &str) -> PathBuf {
    world.join("target/trantor").join(name).join("platform")
}

/// The module names a composed platform's `exposes [...]` lists. codegen
/// writes it on one line.
fn exposes(main: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(main).map_err(|e| format!("read {}: {e}", main.display()))?;
    let line = text.lines().find(|l| l.trim_start().starts_with("exposes"))
        .ok_or_else(|| format!("{} has no exposes list", main.display()))?;
    let inner = line.split_once('[').and_then(|(_, r)| r.split_once(']')).map(|(i, _)| i).unwrap_or("");
    Ok(inner.split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect())
}
