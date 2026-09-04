//! hematite — a build-time component composition system for Roc platforms.
//!
//! v1 subcommand:
//!   hematite compose <world-dir> [--out <dir>]
//!     Read <world-dir>/world.toml + interfaces + components, generate the
//!     composed platform's files into <out> (default: <world-dir>).
//!
//! Only the composition-specific files are generated (main.roc, the workspace,
//! the driver crate, the abi wrapper); interface binding modules and pure-Roc
//! components are copied verbatim (D13). `roc glue` and `cargo`/`roc build` are
//! left to the build step (see the fixture's build.sh) — hematite emits sources.

mod codegen;
mod manifest;
mod publish;
mod resolve;

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
        .ok_or("usage: hematite <compose|publish|tier> <world-dir> [flags]")?;
    let dir = PathBuf::from(it.next().ok_or("missing <world-dir>")?);

    match cmd.as_str() {
        "compose" => {}
        "publish" => return publish::publish(&dir),
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
    let driver = manifest::load_driver(&dir, &world.world.driver)?;
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
