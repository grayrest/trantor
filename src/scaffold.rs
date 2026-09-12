//! `trantor new` and `trantor new-interface` (plan U1 P2/P5).
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

use std::path::Path;

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
         trantor-abi = {{ path = \"target/trantor/{world}/abi\" }}\n"
    )
}

/// What a composed platform requires of its app: `main!`'s signature as the
/// platform declares it, and the modules that signature's types come from.
pub struct MainContract {
    pub signature: String,
    pub imports: Vec<String>,
    /// `|{}|` or `|_args|`, whichever the signature takes.
    pub param: &'static str,
}

/// Read the contract from a composed `platform/main.roc`. The signature is the
/// driver's to decide — trantor-cli's takes `List(OsStr)`, a bare driver's
/// takes `{}` — so a scaffold that writes one from memory is wrong for every
/// baseline but the one it remembered, and this one was: it still wrote
/// `{} => Try({}, [Exit(I32), ..])` after trantor-cli's contract had moved on.
pub fn main_contract(platform: &str) -> Result<MainContract, String> {
    let signature = platform
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("main! :"))
        .ok_or("the composed platform requires no `main!`")?
        .to_string();
    let exposed: Vec<&str> = platform
        .lines()
        .find(|l| l.trim_start().starts_with("exposes"))
        .and_then(|l| l.split_once('[').and_then(|(_, r)| r.split_once(']')))
        .map(|(inner, _)| inner.split(',').map(str::trim).filter(|m| !m.is_empty()).collect())
        .unwrap_or_default();
    let mut imports: Vec<String> = signature
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && exposed.contains(w))
        .map(|w| format!("pf.{w}"))
        .collect();
    imports.dedup();
    let takes_unit = signature.split_once(':').is_some_and(|(_, t)| t.trim_start().starts_with("{}"));
    Ok(MainContract { signature, imports, param: if takes_unit { "{}" } else { "_args" } })
}

fn app_main(name: &str, contract: &MainContract) -> String {
    format!(
        "app [main!] {{ pf: platform \"../target/trantor/{name}/platform/main.roc\" }}\n\n{}\n\n{}\nmain! = |{}| {{\n\tOk({{}})\n}}\n",
        contract.imports.iter().map(|i| format!("import {i}")).collect::<Vec<_>>().join("\n"),
        contract.signature,
        contract.param,
    )
}

/// Write `app/main.roc` for a project whose platform is composed, if it has
/// none yet. The signature comes from the platform, so the app typechecks
/// against whatever baseline the project actually has.
pub fn ensure_app(dir: &Path, name: &str) -> Result<bool, String> {
    let app = dir.join("app/main.roc");
    if app.exists() {
        return Ok(false);
    }
    let platform = dir.join("target/trantor").join(name).join("platform/main.roc");
    let text = std::fs::read_to_string(&platform).map_err(|e| format!("read {}: {e}", platform.display()))?;
    write_new(&app, &app_main(name, &main_contract(&text)?))?;
    Ok(true)
}

fn write_new(p: &Path, body: &str) -> Result<(), String> {
    if p.exists() {
        return Err(format!("{} already exists", p.display()));
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("create {}: {e}", d.display()))?;
    }
    std::fs::write(p, body).map_err(|e| format!("write {}: {e}", p.display()))
}

/// `trantor new <dir> [--from <path|org/repo>]`.
pub fn new_project(dir: &Path, from: Option<&str>) -> Result<(), String> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("new: <dir> has no name")?;
    if dir.join("world.toml").exists() {
        return Err(format!("{} is already a trantor project", dir.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let dep = match from {
        // A local baseline: recorded directly, no lock entry, no network.
        Some(f) if f.contains('/') && Path::new(f).exists() => {
            format!("\n[deps]\nbase = {{ path = {f:?} }}\n")
        }
        Some(_) | None => String::new(),
    };
    write_new(
        &dir.join("world.toml"),
        &format!(
            "[world]\nname = {name:?}\n{}",
            if dep.is_empty() {
                "\n# A baseline supplies the driver, the interfaces and the wiring:\n\
                 #   trantor add <org>/<repo>\n"
            } else {
                &dep
            }
        ),
    )?;
    write_new(&dir.join("Cargo.toml"), &workspace_toml(&name))?;
    write_new(
        &dir.join(".gitignore"),
        "# Everything trantor generates lives here (D-H7-38).\ntarget/\n",
    )?;

    // A github baseline has to be resolved and fetched before anything can
    // compose, so it goes through the same path `trantor add` does.
    if let Some(f) = from {
        if !Path::new(f).exists() {
            // `new` builds right after, so the compose that would validate this
            // happens there; fetching and pinning is all that is needed here.
            let fetched = crate::registry::fetch(f)?;
            crate::registry::record(dir, "world.toml", "base", &fetched)?;
        }
    }

    eprintln!("trantor: created {}", dir.display());
    if from.is_some() {
        // Compose now: `cargo add` and rust-analyzer need the abi crate the
        // patch points at, and it does not exist until something composes.
        crate::build::build(dir, "world.toml", None, "app", "arm64mac")?;
        ensure_app(dir, &name)?;
        eprintln!("trantor: composed — `trantor run` builds it, `cargo add` works in your components");
    } else {
        // No baseline, so no driver has said what `main!` must be, and an app
        // written from memory is wrong for every baseline but one. Generating
        // it stays in `new`: `trantor new --from` is the command that writes it.
        eprintln!(
            "trantor: no baseline, so no app/main.roc: its main! is whatever the baseline's driver requires. \
             `trantor new <dir> --from <org>/<repo>` scaffolds a project with one.",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLI: &str = "platform \"\"\n\trequires {\n\t\tmain! : List(OsStr) => Try({}, [Io(IOErr), ..])\n\t}\n\texposes [Cli, IOErr, OsStr, Stdout]\n";
    const BARE: &str = "platform \"\"\n\trequires {\n\t\tmain! : {} => Try({}, [Exit(I32), ..])\n\t}\n\texposes [Stdout]\n";

    #[test]
    fn the_scaffold_takes_main_from_the_platform_it_was_composed_on() {
        let app = app_main("myapp", &main_contract(CLI).unwrap());
        assert!(app.contains("main! : List(OsStr) => Try({}, [Io(IOErr), ..])\nmain! = |_args| {"), "{app}");
        assert!(app.contains("import pf.OsStr") && app.contains("import pf.IOErr"), "{app}");
        let bare = app_main("myapp", &main_contract(BARE).unwrap());
        assert!(bare.contains("main! : {} => Try({}, [Exit(I32), ..])\nmain! = |{}| {"), "{bare}");
        assert!(!bare.contains("import pf.I32"), "a builtin type is not a platform module: {bare}");
        let none = app_main("myapp", &main_contract("\trequires {\n\t\tmain! : {} => Try({}, [Exit(I32), ..])\n\t}\n\texposes []\n").unwrap());
        assert!(!none.contains("import"), "an empty exposes list imports nothing: {none}");
    }

    #[test]
    fn ensure_app_writes_once_and_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("trantor-scaffold-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let platform = dir.join("target/trantor/myapp/platform");
        std::fs::create_dir_all(&platform).unwrap();
        std::fs::write(platform.join("main.roc"), CLI).unwrap();
        assert!(ensure_app(&dir, "myapp").unwrap(), "no app yet, so one is written");
        std::fs::write(dir.join("app/main.roc"), "mine").unwrap();
        assert!(!ensure_app(&dir, "myapp").unwrap());
        assert_eq!(std::fs::read_to_string(dir.join("app/main.roc")).unwrap(), "mine", "a user's app is never replaced");
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// `trantor new-interface <name>` — the scaffolding `interface-stub` fills in.
pub fn new_interface(dir: &Path, world_file: &str, name: &str) -> Result<(), String> {
    let module: String = {
        let mut c = name.split(['-', '_']).filter(|s| !s.is_empty()).map(|s| {
            let mut ch = s.chars();
            ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default()
        });
        c.next().unwrap_or_default() + &c.collect::<String>()
    };
    if module.is_empty() {
        return Err(format!("new-interface: {name:?} is not a usable interface name"));
    }
    let comp = format!("{name}-host");
    let idir = dir.join("interfaces").join(name);
    write_new(
        &idir.join("interface.toml"),
        &format!(
            "module = {module:?}\n\n# One entry per hosted function. `leaf` is what Roc calls;\n\
             # `symbol_stem` becomes trantor__{}__<stem> in Rust.\n\
             [[hosted]]\nleaf = \"do_it!\"\nsymbol_stem = \"do_it\"\n",
            crate::resolve::sanitize(&comp)
        ),
    )?;
    write_new(
        &idir.join(format!("{module}.roc")),
        &format!("{module} :: [].{{\n\tdo_it! : Str => Str\n}}\n"),
    )?;
    let cdir = dir.join("components").join(&comp);
    write_new(
        &cdir.join("Cargo.toml"),
        &format!(
            "[package]\nname = {comp:?}\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\
             [lib]\ncrate-type = [\"staticlib\", \"rlib\"]\n\n[dependencies]\n\
             # Generated by trantor; the workspace `[patch.crates-io]` resolves it.\n\
             trantor-abi = \"0.0.0\"\n"
        ),
    )?;
    // A [lib] with no source is a manifest cargo cannot even parse, so `cargo
    // add` and `cargo metadata` fail here before the user has done anything
    // wrong. `interface-stub` replaces this file wholesale — comments and
    // nothing else is exactly what it recognises as safe to overwrite, so
    // keep it that way (`stub::is_scaffolding`).
    write_new(
        &cdir.join("src/lib.rs"),
        &format!(
            "//! Host implementation of interface `{name}`.\n             //! Run `trantor interface-stub {name}` to replace this with the\n             //! signatures generated from interfaces/{name}/{module}.roc.\n"
        ),
    )?;

    // Wire it up, preserving the manifest's own formatting and comments.
    let wp = dir.join(world_file);
    let text = std::fs::read_to_string(&wp).map_err(|e| format!("read {}: {e}", wp.display()))?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("parse {}: {e}", wp.display()))?;
    for key in ["interfaces", "components", "wiring"] {
        if let Some(t) = doc[key].or_insert(toml_edit::table()).as_table_mut() {
            t.set_implicit(true);
        }
    }
    doc["interfaces"][name] = toml_edit::table();
    let mut c = toml_edit::table();
    c["kind"] = toml_edit::value("host");
    c["lang"] = toml_edit::value("rust");
    let mut ex = toml_edit::Array::new();
    ex.push(name);
    c["exports"] = toml_edit::value(ex);
    doc["components"][&comp] = c;
    doc["wiring"][name] = toml_edit::value(comp.clone());
    // The app must be able to import it.
    let exports = doc["world"]["exports"].or_insert(toml_edit::value(toml_edit::Array::new()));
    if let Some(a) = exports.as_array_mut() {
        if !a.iter().any(|v| v.as_str() == Some(module.as_str())) {
            a.push(module.as_str());
        }
    }
    std::fs::write(&wp, doc.to_string()).map_err(|e| format!("write {}: {e}", wp.display()))?;

    // And into the user's own cargo workspace, so the editor sees the crate.
    let cargo = dir.join("Cargo.toml");
    if let Ok(t) = std::fs::read_to_string(&cargo) {
        if let Ok(mut d) = t.parse::<toml_edit::DocumentMut>() {
            if let Some(m) = d["workspace"]["members"].as_array_mut() {
                let entry = format!("components/{comp}");
                if !m.iter().any(|v| v.as_str() == Some(entry.as_str())) {
                    m.push(entry.as_str());
                    let _ = std::fs::write(&cargo, d.to_string());
                }
            }
        }
    }

    eprintln!(
        "trantor: added interface `{name}` (module {module}) and component `{comp}`.\n\
         trantor: edit interfaces/{name}/{module}.roc, then `trantor interface-stub {name}` \
         for the Rust signatures."
    );
    Ok(())
}
