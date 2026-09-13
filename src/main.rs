//! trantor — a build-time component composition system for Roc platforms.
//!
//! Subcommands:
//!   trantor compose <world-dir> [--out <dir>] [--world <w>]
//!     Read <world-dir>/world.toml + interfaces + components, generate the
//!     composed platform's files into <out> (default: <world-dir>). Only the
//!     composition-specific files are generated (main.roc, the workspace, the
//!     driver crate, the abi wrapper); interface binding modules and pure-Roc
//!     components are copied verbatim (D13) — compose emits sources only.
//!   trantor build <world-dir> [--app <dir>] [--out <name>] [--world <w>] [--target <t>]
//!     The full pipeline: compose + roc glue + cargo + stage + framework
//!     sysroot + the H0c symbol scan + roc check + roc build. This is the tool
//!     driving the toolchain (superseding the fixtures' build.sh); the scan
//!     runs between cargo and the link (see build.rs). `--target wasm32`
//!     builds every component for wasm32 and merges them into one host.wasm
//!     (see wasm.rs).
//!   trantor scan <world-dir>   — the H0c archive symbol-collision scan alone.
//!   trantor publish / tier     — baseline packaging + tier classification.
//!
//! Service components (plan 2026-09-09 H7): an interface with `kind =
//! "service"` ships its command union (and event/env modules); compose splices
//! one wrapper per service into the driver's marked Cmd/Event/Env modules and
//! generates `abi/src/services.rs`, the driver's typed view of every service's
//! contract (see splice.rs, services.rs).

mod add;
mod build;
mod cargo;
mod codegen;
mod deps;
mod help;
mod interrupt;
mod journal;
mod main_contract;
mod manifest;
mod new_interface;
mod bounded;
mod package_modules;
mod package_suites;
mod package_test;
mod package_worlds;
mod pins;
mod project;
mod publish;
mod readme_claims;
mod readme_examples;
mod readme_generate;
mod readme_lex;
mod readme_taint;
mod registry;
mod resolve;
mod scaffold;
mod scan;
mod services;
mod shipped_modules;
mod splice;
mod stub;
mod symbols;
mod wasm;

use std::path::PathBuf;
use std::process::ExitCode;

/// Rust sets SIGPIPE to SIG_IGN before `main`, so a write to a closed pipe
/// returns EPIPE and `println!` panics on it — `trantor tier | head -1` printed
/// a panic and a backtrace note where `cat` and `grep` print nothing. Restore
/// the default disposition, which is what every other CLI in a pipeline does.
///
/// This does not make `| grep -q` succeed under `set -o pipefail`: the process
/// is then killed by the signal and the shell reports 141, exactly as it would
/// for `sort | head`. A script that wants the output must capture it.
#[cfg(unix)]
fn restore_sigpipe_default() {
    // SAFETY: called once, before any thread is spawned; SIG_DFL is the
    // disposition the process started with before Rust's runtime changed it.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL); }
}
#[cfg(not(unix))]
fn restore_sigpipe_default() {}

fn main() -> ExitCode {
    restore_sigpipe_default();
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("trantor: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The one thing every CLI is asked for first. `trantor --help`, `-h` and
/// `help` all used to print `trantor: missing <world-dir>` and nothing else.

fn run(args: &[String]) -> Result<(), String> {
    let mut it = args.iter().skip(1);
    let Some(cmd) = it.next() else {
        eprint!("{}", help::HELP);
        return Err("no subcommand".into());
    };
    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        print!("{}", help::HELP);
        return Ok(());
    }
    match cmd.as_str() {
        "add" => return add::command(&mut it),
        "update" => return pins::update_command(&mut it),
        "remove" => return pins::remove_command(&mut it),
        _ => {}
    }
    let dir = PathBuf::from(it.next().ok_or_else(|| {
        format!("{cmd}: missing <dir> (the project or world directory). `trantor --help` lists every command.")
    })?);
    // An `add`, `update` or `remove` killed mid-check left its edit behind;
    // whatever runs next in that project puts the files back first.
    if dir.is_dir() {
        journal::recover_noting(&dir)?;
    }

    match cmd.as_str() {
        "check" | "run" | "test" => {
            let mut world_file = String::from("world.toml");
            let mut world_given = false;
            let mut app = String::from("app");
            let mut target = String::from("arm64mac");
            let mut rest: Vec<String> = Vec::new();
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => {
                        world_file = it.next().ok_or("--world: missing file")?.clone();
                        world_given = true;
                    }
                    "--app" => app = it.next().ok_or("--app: missing dir")?.clone(),
                    "--target" => target = it.next().ok_or("--target: missing triple")?.clone(),
                    // Everything after `--` belongs to the app, not to trantor.
                    "--" => rest.extend(it.by_ref().cloned()),
                    other => return Err(format!("unknown flag {other:?}")),
                }
            }
            return match cmd.as_str() {
                "check" => build::check(&dir, &world_file, &app),
                // A package has no world of its own: `trantor test` composes
                // it against its [dev-deps] and runs the package checks (T3).
                // package.toml wins unless --world names a world explicitly: a
                // stray world.toml in a package used to make this test nothing.
                "test" if !world_given && dir.join("package.toml").exists() => {
                    package_test::test_package(&dir)
                }
                "test" => build::test(&dir, &world_file, &app),
                _ => {
                    let status = build::run_app(&dir, &world_file, &app, &target, &rest)?;
                    // The app's exit code is the app's, not a build result.
                    std::process::exit(status.code().unwrap_or(70));
                }
            };
        }
        "new" => {
            let mut from: Option<String> = None;
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--from" => from = Some(it.next().ok_or("--from: missing <path|org/repo>")?.clone()),
                    "--cli" => {}  // the only app shape today; accepted so the
                                   // walkthrough reads the way it will later.
                    other => return Err(format!("unknown flag {other:?}")),
                }
            }
            return scaffold::new_project(&dir, from.as_deref());
        }
        "new-interface" => {
            let name = it.next().ok_or("new-interface: missing <name>")?.clone();
            let mut world_file = String::from("world.toml");
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
                    other => return Err(format!("unknown flag {other:?}")),
                }
            }
            return new_interface::new_interface(&dir, &world_file, &name);
        }
        "compose" => {}
        "interface-stub" => {
            // D-U1-7: the Rust signature for a hosted interface, from the Roc
            // declaration and the generated glue, instead of guessed.
            let iface = it.next().ok_or("interface-stub: missing <interface>")?.clone();
            let mut world_file = String::from("world.toml");
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
                    other => return Err(format!("unknown flag {other:?}")),
                }
            }
            return stub::interface_stub(&dir, &world_file, &iface);
        }
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
            let (tier, examined) = publish::classify(&world)?;
            match tier {
                publish::Tier::One => {
                    println!("Tier 1: pure-Roc extension — reuses the baseline's prebuilt archives, no Rust toolchain (glue + libhost unchanged). Add module + edit exposes/import.");
                }
                publish::Tier::Two(hosts) => {
                    println!("Tier 2: adds host component(s) {hosts:?} — new hosted symbols, so full source composition (cargo + roc glue) is required. This crosses the tier cliff (D11).");
                }
            }
            println!("({examined} component(s) examined)");
            return Ok(());
        }
        other => return Err(format!("unknown subcommand {other:?}")),
    }
    let mut out: Option<PathBuf> = None;
    let mut world_file = String::from("world.toml");
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out: missing path")?)),
            "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    build::compose(&dir, &world_file, out)
}

