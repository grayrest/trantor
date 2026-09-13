//! `trantor new` (plan U1 P2).
//!
//! The point of `new` is not the file count — that metric was replaced
//! (D-U1-15) after it started forbidding the two files a project most needs.
//! The point is that everything works the moment it exists: `trantor run`
//! builds, and `cargo add` / `cargo check` / rust-analyzer resolve inside the
//! component where the user writes Rust.
//!
//! That second half is the one that does not come free (D-U1-11). A component's
//! `trantor-abi` dependency can only be satisfied by a crate trantor GENERATES,
//! so before the first compose there is nothing for cargo to resolve — measured
//! on a clean checkout of the cargo-root fixture, where `cargo metadata` exits
//! 101 and rust-analyzer therefore loads nothing. `new` composes as part of
//! scaffolding for exactly that reason, and every other command composes first
//! too, so any of them repairs a fresh clone.

use std::path::{Path, PathBuf};

use crate::main_contract::{app_main, main_contract};
use crate::new_marker::Marker;

/// The workspace root a scaffolded project gets. `[patch.crates-io]` is what
/// makes `trantor-abi = "0.0.0"` resolve for the user's own crates in the
/// SOURCE tree; trantor's build uses the generated workspace under `target/`,
/// which carries its own. `exclude` keeps cargo from trying to claim that one.
fn workspace_toml(world: &str) -> String {
    format!(
        "# Your crates' workspace. trantor composes a second one under target/;\n\
         # this is the one your editor and `cargo add` see.\n\
         [workspace]\n\
         resolver = \"2\"\n\
         members = []\n\
         exclude = [\"target\"]\n\
         \n\
         # `trantor-abi` is generated, so it has no registry to come from. Every\n\
         # component depends on the name `trantor-abi = \"0.0.0\"` and this points\n\
         # it at the composed crate. The path only exists after a compose, which\n\
         # is why `trantor new` runs one.\n\
         [patch.crates-io]\n\
         trantor-abi = {{ path = {} }}\n",
        crate::package_worlds::toml_str(&format!("target/trantor/{world}/abi"))
    )
}

/// Write `app/main.roc` for a project whose platform is composed, if it has
/// none yet. The signature comes from the platform, so the app typechecks
/// against whatever baseline the project actually has.
pub fn ensure_app(dir: &Path, name: &str) -> Result<App, String> {
    let app = dir.join("app/main.roc");
    if app.exists() {
        return Ok(App::Exists);
    }
    let platform = dir.join("target/trantor").join(name).join("platform/main.roc");
    let text = std::fs::read_to_string(&platform).map_err(|e| format!("read {}: {e}", platform.display()))?;
    match main_contract(&text) {
        Ok(contract) => write_new(&app, &app_main(name, &contract)).map(|()| App::Written),
        Err(why) => Ok(App::NoShape(why)),
    }
}

pub enum App {
    Written,
    Exists,
    /// The platform's contract is not one the scaffold can write, and why.
    NoShape(String),
}

pub fn write_new(p: &Path, body: &str) -> Result<(), String> {
    if p.exists() {
        return Err(format!("{} already exists", p.display()));
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("create {}: {e}", d.display()))?;
    }
    std::fs::write(p, body).map_err(|e| format!("write {}: {e}", p.display()))
}

/// `trantor new <dir> [--from <path|org/repo>]`. A `new` that fails removes
/// what it wrote: it used to leave a world.toml behind, and then refuse to run
/// again in a directory it said was "already a trantor project". A killed one
/// is cleaned up by the next (`new_marker`).
pub fn new_project(dir: &Path, from: Option<&str>) -> Result<(), String> {
    let mut marker = Marker::claim(dir)?;
    if dir.join("world.toml").exists() {
        marker.finish();
        return Err(format!("{} is already a trantor project", dir.display()));
    }
    match scaffold(dir, from, &mut marker) {
        Ok(()) => {
            marker.finish();
            Ok(())
        }
        Err(e) => {
            let note = match marker.undo() {
                Ok(()) => "new left nothing behind — fix that and run it again".to_string(),
                Err(u) => u,
            };
            Err(format!("{e}\n({note})"))
        }
    }
}

fn scaffold(dir: &Path, from: Option<&str>, marker: &mut Marker) -> Result<(), String> {
    // Composition keys output by the directory's resolved name, so a project
    // reached through a symlink is staged under the real one — and the
    // workspace patch and the app header have to name that one too.
    let name = dir
        .canonicalize()
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .ok_or("new: <dir> has no name")?;
    // Roc loads the platform path in the app header as written, escapes and
    // all, so a name it cannot spell literally is refused, not escaped.
    if name.contains(['"', '\\']) || name.contains("${") || name.chars().any(char::is_control) {
        return Err(format!("new: {name:?} cannot be a project directory's name: an app header could not name its platform"));
    }
    let baseline = match from {
        None => Baseline::None,
        // An existing directory is a local baseline, however it is spelled;
        // recorded relative to the project, since that is where it is read from.
        Some(f) if Path::new(f).is_dir() => Baseline::Path(relative_to(Path::new(f), dir)?),
        // Spelled like a path but not a directory: a typo, not a github repo.
        Some(f) if looks_like_path(f) => return Err(format!("new --from {f:?}: there is no directory there")),
        Some(f) if f.split('/').count() == 2 && !f.split('/').any(str::is_empty) => Baseline::GitHub(f.to_string()),
        Some(f) => return Err(format!("new --from {f:?}: neither a directory nor <org>/<repo>")),
    };
    let body = match &baseline {
        Baseline::None => "\n# A baseline supplies the driver, the interfaces and the wiring:\n#   trantor add <org>/<repo>\n".to_string(),
        Baseline::Path(p) => format!("\n[deps]\nbase = {{ path = {} }}\n", crate::package_worlds::toml_str(p)),
        Baseline::GitHub(_) => String::new(),
    };
    write_new(&dir.join("world.toml"), &format!("[world]\nname = {}\n{body}", crate::package_worlds::toml_str(&name)))?;
    marker.wrote("world.toml")?;
    write_new(&dir.join("Cargo.toml"), &workspace_toml(&name))?;
    marker.wrote("Cargo.toml")?;
    write_new(&dir.join(".gitignore"), "# Everything trantor generates lives here (D-H7-38).\ntarget/\n# An interrupted add/update/remove's journal (D-T3-12).\n.trantor-edit/\n# A running or interrupted `trantor new`'s marker.\n.trantor-new\n")?;
    marker.wrote(".gitignore")?;
    if let Baseline::GitHub(slug) = &baseline {
        // `new` builds right after, so the compose that would validate this
        // happens there; fetching and pinning is all that is needed here.
        let fetched = crate::registry::fetch(slug)?;
        crate::registry::record(dir, "world.toml", "base", &fetched)?;
        marker.wrote("world.toml")?;
        marker.wrote(crate::registry::LOCK_FILE)?;
    }

    eprintln!("trantor: created {}", dir.display());
    if let Baseline::None = baseline {
        // No baseline, so no driver has said what `main!` must be, and an app
        // written from memory is wrong for every baseline but one.
        eprintln!(
            "trantor: no baseline, so no app/main.roc: its main! is whatever the baseline's driver requires. \
             `trantor new <dir> --from <org>/<repo>` scaffolds a project with one.",
        );
        return Ok(());
    }
    // Compose now: `cargo add` and rust-analyzer need the abi crate the patch
    // points at, and it does not exist until something composes.
    marker.creating_dir("target")?;
    crate::build::build(dir, "world.toml", None, "app", "arm64mac")?;
    marker.creating_dir("app")?;
    let app = ensure_app(dir, &name)?;
    if let App::Written = app {
        marker.wrote("app/main.roc")?;
    }
    match app {
        // A driver whose contract is not a lone `main!` still composed: the
        // project is usable, it just needs the app written by hand.
        App::NoShape(why) => eprintln!("trantor: composed, but wrote no app/main.roc: {why}"),
        App::Written | App::Exists => {
            eprintln!("trantor: composed — `trantor run` builds it, `cargo add` works in your components")
        }
    }
    Ok(())
}

/// A `--from` only a path could mean. `org/repo` stays a slug even when a
/// directory named `org` is here: sibling checkouts are the common layout.
fn looks_like_path(f: &str) -> bool {
    f.starts_with(['.', '/', '~']) || f.split('/').count() != 2 || f.ends_with(".toml")
}

enum Baseline {
    None,
    Path(String),
    GitHub(String),
}

/// `target` as a path from `base`, both resolved.
fn relative_to(target: &Path, base: &Path) -> Result<String, String> {
    let t = target.canonicalize().map_err(|e| format!("{}: {e}", target.display()))?;
    let b = base.canonicalize().map_err(|e| format!("{}: {e}", base.display()))?;
    let (tc, bc): (Vec<_>, Vec<_>) = (t.components().collect(), b.components().collect());
    let common = tc.iter().zip(&bc).take_while(|(x, y)| x == y).count();
    let mut rel = PathBuf::new();
    for _ in common..bc.len() {
        rel.push("..");
    }
    for c in &tc[common..] {
        rel.push(c);
    }
    Ok(rel.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLI: &str = "platform \"\"\n\trequires {\n\t\tmain! : List(OsStr) => Try({}, [Io(IOErr), ..])\n\t}\n\texposes [Cli, IOErr, OsStr, Stdout]\n";

    #[test]
    fn ensure_app_writes_once_and_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("trantor-scaffold-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let platform = dir.join("target/trantor/myapp/platform");
        std::fs::create_dir_all(&platform).unwrap();
        std::fs::write(platform.join("main.roc"), CLI).unwrap();
        assert!(matches!(ensure_app(&dir, "myapp").unwrap(), App::Written), "no app yet, so one is written");
        std::fs::write(dir.join("app/main.roc"), "mine").unwrap();
        assert!(matches!(ensure_app(&dir, "myapp").unwrap(), App::Exists));
        assert_eq!(std::fs::read_to_string(dir.join("app/main.roc")).unwrap(), "mine", "a user's app is never replaced");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_relative_baseline_is_recorded_from_the_project_not_the_cwd() {
        let t = std::env::temp_dir().join(format!("trantor-rel-{}", std::process::id()));
        std::fs::create_dir_all(t.join("base")).unwrap();
        std::fs::create_dir_all(t.join("sub/p")).unwrap();
        assert_eq!(relative_to(&t.join("base"), &t.join("sub/p")).unwrap(), "../../base");
        std::fs::remove_dir_all(&t).ok();
    }
}
