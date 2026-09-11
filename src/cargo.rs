//! Building the component archives with cargo — in the world's own generated
//! workspace, or inside a HOST workspace that already owns the crates
//! (D-H7-14: `[world] cargo_root`).
//!
//! A crate belongs to exactly one cargo workspace. A `path` component such as
//! roc-solid's `crates/host-im` is a member of that repo's root workspace, so
//! trantor cannot also list it in a per-world workspace, and several worlds
//! (clay, dom, colorhunt…) name the same crates. With `cargo_root` set,
//! trantor emits no workspace of its own and instead runs, per component,
//!
//!   cargo --config 'patch.crates-io.trantor-abi.path="<world>/abi"' \
//!         build --release -p <package>
//!
//! in the host workspace. Components declare `trantor-abi = "0.0.0"` (a
//! crates-io name that does not exist) and the host's root `Cargo.toml`
//! carries a default `[patch.crates-io]` to its baseline world's abi so its
//! own `cargo clippy/test --workspace` keep resolving; the per-build patch
//! swaps in each world's generated abi. Measured (2026-09-09): the override
//! takes effect per build and `Cargo.lock` does not churn between worlds.

use crate::manifest::{component_dir, World};
use crate::resolve::{sanitize, Resolved};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The abi package name every component depends on (`[dependencies]
/// trantor-abi = "0.0.0"` under `cargo_root`; a path dep otherwise).
const ABI_PACKAGE: &str = "trantor-abi";

#[derive(Deserialize)]
struct CargoManifest {
    package: CargoPackage,
}
#[derive(Deserialize)]
struct CargoPackage {
    name: String,
}

/// The `[package] name` of a crate.
pub fn package_name(cargo_toml: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(cargo_toml).map_err(|e| format!("read {}: {e}", cargo_toml.display()))?;
    let m: CargoManifest = toml::from_str(&text).map_err(|e| format!("parse {}: {e}", cargo_toml.display()))?;
    Ok(m.package.name)
}

/// Build every non-Roc component archive. `wasm_triple` selects a cross
/// target (with `panic=abort`, since wasm has no unwinder). Returns, in
/// `archive_order`, each component and the archive cargo produced for it.
pub fn build(
    dir: &Path,
    gen: &Path,
    world: &World,
    r: &Resolved,
    wasm_triple: Option<&str>,
) -> Result<Vec<(String, PathBuf)>, String> {
    let profile_dir = |root: &Path| match wasm_triple {
        Some(t) => root.join("target").join(t).join("release"),
        None => root.join("target").join("release"),
    };
    let target_args = |args: &mut Vec<String>| {
        if let Some(t) = wasm_triple {
            args.push("--target".into());
            args.push(t.into());
        }
    };
    // What this world wires, for a driver that serves several worlds (D-H7-27):
    // a `build.rs` turns `TRANTOR_SERVICES` into `cfg(trantor_service = "…")`
    // so world-conditional driver code — an `Env` block only an audio world
    // has — compiles in every world. Sorted, so the value is stable.
    let mut wired: Vec<&str> = world.wiring.keys().map(String::as_str).collect();
    wired.sort_unstable();
    let services_env = wired.join(",");
    let size_correct = wasm_triple.is_some() && world.world.wasm_size_correct;
    let run = |args: &[String], cwd: &Path| -> Result<(), String> {
        let mut cmd = Command::new("cargo");
        cmd.args(args).current_dir(cwd);
        cmd.env("TRANTOR_WORLD", &world.world.name);
        cmd.env("TRANTOR_SERVICES", &services_env);
        if wasm_triple.is_some() {
            cmd.env("CARGO_PROFILE_RELEASE_PANIC", "abort");
        }
        if size_correct {
            // The D25 recipe, as environment overrides: a `[profile]` block in
            // a workspace member is ignored, and `cargo-features =
            // ["panic-immediate-abort"]` would make the whole workspace fail
            // to parse on stable.
            cmd.env("RUSTC_BOOTSTRAP", "1")
                .env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "z")
                .env("CARGO_PROFILE_RELEASE_LTO", "true")
                .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1")
                .env("CARGO_PROFILE_RELEASE_STRIP", "true")
                .env("RUSTFLAGS", "-Zunstable-options -Cpanic=immediate-abort");
        }
        let status = cmd.status().map_err(|e| format!("cargo build: spawn: {e}"))?;
        if !status.success() {
            return Err(format!("cargo build: cargo exited {status}"));
        }
        Ok(())
    };

    let Some(cargo_root) = &world.world.cargo_root else {
        // The world's own generated workspace: one build, archives named by
        // the sanitized component name (cargo replaces `-` the same way).
        //
        // Run in `gen`, not the world dir: the workspace manifest is generated
        // (D-H7-38), so the world dir holds no Cargo.toml and cargo would walk
        // UP out of the fixture and adopt whatever workspace it found first.
        let mut args = vec!["build".to_string(), "--release".to_string()];
        target_args(&mut args);
        run(&args, gen)?;
        let built = profile_dir(gen);
        return Ok(r
            .archive_order
            .iter()
            .map(|c| (c.clone(), built.join(format!("lib{}.a", sanitize(c)))))
            .collect());
    };

    // The host workspace: per-package builds with this world's abi patched in.
    let root = dir.join(cargo_root);
    // The world's abi crate is generated, so it lives under `gen`, not beside
    // the world file (D-H7-38).
    let abi = gen
        .join("abi")
        .canonicalize()
        .map_err(|e| format!("canonicalize {}/abi: {e}", gen.display()))?;
    let patch = format!("patch.crates-io.{ABI_PACKAGE}.path=\"{}\"", abi.display());
    let built = profile_dir(&root);
    let mut out = Vec::new();
    for comp in &r.archive_order {
        let c = &world.components[comp];
        let pkg = package_name(&component_dir(dir, comp, c).join("Cargo.toml"))?;
        // `cargo rustc … --crate-type staticlib -Z build-std` for the
        // size-correct wasm build, plain `cargo build` otherwise.
        let mut args = vec!["--config".to_string(), patch.clone()];
        if size_correct {
            args.extend(["rustc", "--release", "-p", &pkg].map(String::from));
        } else {
            args.extend(["build", "--release", "-p", &pkg].map(String::from));
        }
        // The HC0 feature knob, as cargo flags rather than a Cargo.toml rewrite:
        // the crate is shared between worlds and must not be edited in place.
        if !c.features.is_empty() {
            args.push("--features".to_string());
            args.push(c.features.join(","));
        }
        if c.default_features == Some(false) {
            args.push("--no-default-features".to_string());
        }
        target_args(&mut args);
        if size_correct {
            args.extend(["--crate-type", "staticlib", "-Z", "build-std=core,alloc,std,panic_abort"].map(String::from));
        }
        run(&args, &root)?;
        // The UPLIFTED path, `target/release/lib<pkg>.a` — which every world's
        // build of the same package overwrites (cargo's `compiler-artifact`
        // message names only this path, not the per-variant `deps/` file).
        // Safe only because the caller holds `build_lock` from here through
        // staging (D-H7-34).
        out.push((comp.clone(), built.join(format!("lib{}.a", pkg.replace('-', "_")))));
    }
    Ok(out)
}

/// An exclusive advisory lock on the host workspace's target directory,
/// held by a caller from its first `cargo build` through its stage copy
/// (D-H7-34). Two worlds composing in parallel in one workspace — roc-solid's
/// gate suite — build the same driver package to the same uplifted path, and
/// whichever finished last was what both staged: a driver with another
/// world's `cfg(trantor_service…)` and `Env` layout. cargo's own lock covers
/// a build, not the copy after it. Released on drop.
pub fn build_lock(dir: &Path, world: &World) -> Result<Option<std::fs::File>, String> {
    let Some(cargo_root) = &world.world.cargo_root else { return Ok(None) };
    let target = dir.join(cargo_root).join("target");
    std::fs::create_dir_all(&target).map_err(|e| format!("mkdir {}: {e}", target.display()))?;
    let path = target.join("trantor-build.lock");
    let f = std::fs::File::create(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
    f.lock().map_err(|e| format!("lock {}: {e}", path.display()))?;
    Ok(Some(f))
}

