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
mod manifest;
mod package_suites;
mod package_test;
mod publish;
mod readme_examples;
mod registry;
mod resolve;
mod scaffold;
mod scan;
mod services;
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
const HELP: &str = "\
trantor — compose Roc platforms from Rust components.

Starting out
  new <dir> [--from <path|org/repo>]  scaffold a project (and compose it)
  add <org/repo> [<dir>] [--as <name>] add a dependency here (world or package),
                                      pinning a semver tag; composes, or changes nothing
  update <dir> [<name>]               move a pin
  remove <dir> <name>                 drop a dependency

Working
  check <dir>                         compose + typecheck (the inner loop)
  run <dir> [-- <args>]               build, then run the app
  test <dir>                          a world: roc expects + cargo tests
                                      a package: composed on its [dev-deps], plus tests/
  build <dir> [--app <d>] [--out <n>] [--target <t>] [--platform-only]

Adding a capability
  new-interface <dir> <name>          scaffold an interface + host component
  interface-stub <dir> <name>         the Rust signatures, from the Roc

Platform authoring
  compose <dir> [--out <d>]           generate sources only
  scan <dir>                          the archive symbol-collision scan
  publish <dir>                       package a baseline
  tier <dir>                          classify an extension

Common flags: --world <file> picks a world variant (default world.toml).

Testing a package (a directory with package.toml, run as `trantor test .`)
  Composes it alone (it must fail naming the driver unless one is in reach),
  then on its [dev-deps]; checks the dev-deps alone expose none of its
  exports; runs its expects; builds README.md's roc blocks (a whole app runs
  as written) and compares each `expr   # value` comment; then each
  tests/<name>/ is one of:
    main.roc + expected   an app, stdout compared line for line
    Cargo.toml            cargo test --release
    test.sh               run with TRANTOR ROC PKG TMP DEPS DEV_DEPS
  A README example's missing bindings come from tests/readme-prelude.roc.
";

fn run(args: &[String]) -> Result<(), String> {
    let mut it = args.iter().skip(1);
    let Some(cmd) = it.next() else {
        eprint!("{HELP}");
        return Err("no subcommand".into());
    };
    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        print!("{HELP}");
        return Ok(());
    }
    if cmd == "add" {
        return add_command(&mut it);
    }
    let dir = PathBuf::from(it.next().ok_or_else(|| {
        format!("{cmd}: missing <dir> (the project or world directory). `trantor --help` lists every command.")
    })?);

    match cmd.as_str() {
        "check" | "run" | "test" => {
            let mut world_file = String::from("world.toml");
            let mut app = String::from("app");
            let mut target = String::from("arm64mac");
            let mut rest: Vec<String> = Vec::new();
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
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
                "test" if !dir.join(&world_file).exists() && dir.join("package.toml").exists() => {
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
            return scaffold::new_interface(&dir, &world_file, &name);
        }
        "update" | "remove" => {
            // D-U1-3. `dir` is the world dir. (`add` parses its own arguments.)
            let mut world_file = String::from("world.toml");
            let mut arg: Option<String> = None;
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--world" => world_file = it.next().ok_or("--world: missing file")?.clone(),
                    other if other.starts_with("--") => return Err(format!("unknown flag {other:?}")),
                    other => arg = Some(other.to_string()),
                }
            }
            return match cmd.as_str() {
                "update" => registry::update(&dir, &world_file, arg.as_deref()),
                _ => registry::remove(&dir, &world_file, &arg.ok_or("remove: missing <name>")?),
            };
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

/// `trantor add <org>/<repo> [<dir>] [--as <name>] [--world <file>]`: the
/// dependency is the first positional, the project directory the optional
/// second (default: the current one).
fn add_command(it: &mut std::iter::Skip<std::slice::Iter<'_, String>>) -> Result<(), String> {
    let (mut positional, mut as_name, mut world): (Vec<String>, Option<String>, Option<String>) = (vec![], None, None);
    while let Some(f) = it.next() {
        match f.as_str() {
            "--as" => as_name = Some(it.next().ok_or("--as: missing name")?.clone()),
            "--world" => world = Some(it.next().ok_or("--world: missing file")?.clone()),
            other if other.starts_with("--") => return Err(format!("unknown flag {other:?}")),
            other => positional.push(other.to_string()),
        }
    }
    let (slug, dir) = match positional.as_slice() {
        [slug] => (slug.clone(), PathBuf::from(".")),
        [first, second] if !is_slug(first) && is_slug(second) => {
            return Err(format!("add: arguments are `trantor add <org>/<repo> [<dir>]` — try `trantor add {second} {first}`"));
        }
        [slug, dir] => (slug.clone(), PathBuf::from(dir)),
        [] => return Err("add: missing <org>/<repo>; usage: trantor add <org>/<repo> [<dir>]".into()),
        _ => return Err("add: expected `trantor add <org>/<repo> [<dir>]`".into()),
    };
    add::add(&dir, world.as_deref(), &slug, as_name.as_deref())
}

fn is_slug(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && !p.starts_with('.')) && !std::path::Path::new(s).exists()
}
