//! A package's `tests/<n>/` directories, each one kind of suite (T3):
//!
//! - `main.roc` + `expected`: an app built on the package, stdout diffed.
//! - `Cargo.toml`: a Rust crate, `cargo test --release`.
//! - `test.sh`: anything a diff cannot express — exit codes, raw argv, peer
//!   processes, timing — given `TRANTOR`, `ROC`, `PKG`, `TMP`, and the
//!   `[deps]` body of a world with the package (`DEPS`) and without (`DEV_DEPS`).
use std::path::Path;
use std::process::Command;

use crate::package_test::{s, trantor_output, Steps};

pub fn run_all(steps: &Steps, with: &Path) -> Result<(), String> {
    let dir = steps.root.join("tests");
    let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(()) };
    let mut suites: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    suites.sort();
    for suite in suites {
        let name = suite.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let kinds = [("main.roc", suite.join("main.roc").is_file()),
                     ("Cargo.toml", suite.join("Cargo.toml").is_file()),
                     ("test.sh", suite.join("test.sh").is_file())];
        let found: Vec<&str> = kinds.iter().filter(|(_, present)| *present).map(|(k, _)| *k).collect();
        match found.as_slice() {
            ["main.roc"] => app(&suite, &name, with)?,
            ["Cargo.toml"] => cargo(&suite, &name)?,
            ["test.sh"] => script(steps, &suite, &name)?,
            [] => return Err(format!("tests/{name}/ is none of: main.roc + expected, Cargo.toml, test.sh")),
            many => return Err(format!("tests/{name}/ is more than one kind of suite: {}", many.join(", "))),
        }
    }
    Ok(())
}

fn app(suite: &Path, name: &str, with: &Path) -> Result<(), String> {
    let expected = std::fs::read_to_string(suite.join("expected"))
        .map_err(|_| format!("tests/{name}/main.roc has no tests/{name}/expected to compare against"))?;
    std::fs::copy(suite.join("main.roc"), with.join("app/main.roc")).map_err(|e| format!("copy tests/{name}/main.roc: {e}"))?;
    let out = trantor_output(&["run", s(with)], with)?;
    let got = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        return Err(format!("tests/{name}: build or run failed\n{got}{}", String::from_utf8_lossy(&out.stderr)));
    }
    if got != expected {
        return Err(format!("tests/{name}: output differs (expected - / got +)\n{}", diff(&expected, &got)));
    }
    println!("ok: tests/{name} — {} lines exact", expected.lines().count());
    Ok(())
}

fn cargo(suite: &Path, name: &str) -> Result<(), String> {
    let manifest = suite.join("Cargo.toml");
    let out = Command::new("cargo")
        .args(["test", "--release", "--manifest-path", s(&manifest)])
        .output()
        .map_err(|e| format!("tests/{name}: spawn cargo: {e}"))?;
    if !out.status.success() {
        return Err(format!("tests/{name}: cargo test failed\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    }
    println!("ok: tests/{name} — cargo test --release");
    Ok(())
}

fn script(steps: &Steps, suite: &Path, name: &str) -> Result<(), String> {
    let tmp = steps.scratch.join(format!("script-{name}"));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    let exe = std::env::current_exe().map_err(|e| format!("locate trantor: {e}"))?;
    let out = Command::new("bash")
        .arg("test.sh")
        .current_dir(suite)
        .env("TRANTOR", &exe)
        .env("ROC", crate::build::roc_bin())
        .env("PKG", steps.root)
        .env("DEPS", steps.deps_body(true)?)
        .env("DEV_DEPS", steps.deps_body(false)?)
        .env("TMP", &tmp)
        .output()
        .map_err(|e| format!("tests/{name}: spawn bash: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        return Err(format!("tests/{name}/test.sh failed\n{said}{}", String::from_utf8_lossy(&out.stderr)));
    }
    let oks: Vec<&str> = said.lines().filter(|l| l.starts_with("ok:")).collect();
    if oks.is_empty() {
        println!("ok: tests/{name}");
    }
    for ok in oks {
        println!("{ok}");
    }
    Ok(())
}

/// A line diff good enough to read a failure by: every differing line pair.
fn diff(expected: &str, got: &str) -> String {
    let (e, g): (Vec<_>, Vec<_>) = (expected.lines().collect(), got.lines().collect());
    let mut out = String::new();
    for i in 0..e.len().max(g.len()) {
        let (a, b) = (e.get(i).copied(), g.get(i).copied());
        if a != b {
            if let Some(a) = a { out.push_str(&format!("    -{a}\n")); }
            if let Some(b) = b { out.push_str(&format!("    +{b}\n")); }
        }
    }
    out
}
