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
    let cmd = it.next().ok_or("usage: hematite compose <world-dir> [--out <dir>]")?;
    if cmd != "compose" {
        return Err(format!("unknown subcommand {cmd:?}; expected `compose`"));
    }
    let dir = PathBuf::from(it.next().ok_or("compose: missing <world-dir>")?);
    let mut out = dir.clone();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--out" => out = PathBuf::from(it.next().ok_or("--out: missing path")?),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }

    let world = manifest::load_world(&dir)?;
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
