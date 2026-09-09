//! hematite — a build-time component composition system for Roc platforms.
//!
//! Subcommands:
//!   hematite compose <world-dir> [--out <dir>] [--world <w>]
//!     Read <world-dir>/world.toml + interfaces + components, generate the
//!     composed platform's files into <out> (default: <world-dir>). Only the
//!     composition-specific files are generated (main.roc, the workspace, the
//!     driver crate, the abi wrapper); interface binding modules and pure-Roc
//!     components are copied verbatim (D13) — compose emits sources only.
//!   hematite build <world-dir> [--app <dir>] [--out <name>] [--world <w>] [--target <t>]
//!     The full pipeline: compose + roc glue + cargo + stage + framework
//!     sysroot + the H0c symbol scan + roc check + roc build. This is the tool
//!     driving the toolchain (superseding the fixtures' build.sh); the scan
//!     runs between cargo and the link (see build.rs). `--target wasm32`
//!     builds every component for wasm32 and merges them into one host.wasm
//!     (see wasm.rs).
//!   hematite scan <world-dir>   — the H0c archive symbol-collision scan alone.
//!   hematite publish / tier     — baseline packaging + tier classification.
//!
//! Service components (plan 2026-09-09 H7): an interface with `kind =
//! "service"` ships its command union (and event/env modules); compose splices
//! one wrapper per service into the driver's marked Cmd/Event/Env modules and
//! generates `abi/src/services.rs`, the driver's typed view of every service's
//! contract (see splice.rs, services.rs).

mod build;
mod cargo;
mod codegen;
mod manifest;
mod publish;
mod resolve;
mod scan;
mod services;
mod splice;
mod symbols;
mod wasm;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hematite: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut it = args.iter().skip(1);
    let cmd = it
        .next()
        .ok_or("usage: hematite <compose|build|publish|tier|scan> <world-dir> [flags]")?;
    let dir = PathBuf::from(it.next().ok_or("missing <world-dir>")?);

    match cmd.as_str() {
        "compose" => {}
        "publish" => return publish::publish(&dir),
        "build" => {
            // The full pipeline: compose + glue + cargo + scan + roc check/build.
            let mut world_file = String::from("world.toml");
            let mut app = String::from("app");
            let mut out = String::from("app");
            let mut target = String::from("arm64mac");
            let mut app_link = true;
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
                    "--app" => app = it.next().ok_or("--app: missing dir")?.clone(),
                    "--out" => out = it.next().ok_or("--out: missing name")?.clone(),
                    "--target" => target = it.next().ok_or("--target: missing triple")?.clone(),
                    // Prepare the platform (through the scan) without linking
                    // an app: for a repo whose gates `roc build` many apps
                    // against one composed platform.
                    "--platform-only" => app_link = false,
                    other => return Err(format!("unknown flag {other:?}")),
                }
            }
            let app = if app_link { Some(app.as_str()) } else { None };
            return build::build(&dir, &world_file, app, &out, &target);
        }
        "scan" => {
            // H0c archive symbol-collision scan. Runs after cargo builds the
            // component archives (unlike compose, which stops at sources).
            let mut world_file = String::from("world.toml");
            let mut targets_dir: Option<PathBuf> = None;
            let mut target = String::from("arm64mac");
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
                    "--targets-dir" => {
                        targets_dir = Some(PathBuf::from(it.next().ok_or("--targets-dir: missing path")?))
                    }
                    "--target" => target = it.next().ok_or("--target: missing triple")?.clone(),
                    other => return Err(format!("unknown flag {other:?}")),
                }
            }
            return scan::scan(&dir, &world_file, targets_dir, &target);
        }
        "tier" => {
            // classify an extension world's additions (D11): Tier 1 (pure-Roc,
            // no toolchain) vs Tier 2 (host code, full source composition).
            let mut world_file = String::from("world.toml");
            while let Some(f) = it.next() {
                if f == "--world" {
                    world_file = it.next().ok_or("--world: missing file")?.clone();
                }
            }
            let world = manifest::load_world(&dir, &world_file)?;
            match publish::classify(&world) {
                publish::Tier::One => {
                    println!("Tier 1: pure-Roc extension — reuses the baseline's prebuilt archives, no Rust toolchain (glue + libhost unchanged). Add module + edit exposes/import.");
                }
                publish::Tier::Two(hosts) => {
                    println!("Tier 2: adds host component(s) {hosts:?} — new hosted symbols, so full source composition (cargo + roc glue) is required. This crosses the tier cliff (D11).");
                }
            }
            return Ok(());
        }
        other => return Err(format!("unknown subcommand {other:?}")),
    }
    let mut out = dir.clone();
    let mut world_file = String::from("world.toml");
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--out" => out = PathBuf::from(it.next().ok_or("--out: missing path")?),
            "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }

    let world = manifest::load_world(&dir, &world_file)?;
    let driver = manifest::load_driver(&dir, &world)?;
    let resolved = resolve::resolve(&dir, &world, &driver)?;
    codegen::emit(&dir, &out, &world, &driver, &resolved)?;

    eprintln!(
        "hematite: composed `{}` -> {} ({} hosted symbols, {} archives, driver `{}`)",
        world.world.name,
        out.display(),
        resolved.hosted.len(),
        resolved.archive_order.len(),
        resolved.driver,
    );
    Ok(())
}
