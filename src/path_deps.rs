//! Re-anchor a copied component manifest's relative `path` dependencies.
//!
//! A component crate is COPIED into the generated workspace
//! (`target/trantor/<world>/components/<name>`, D-H7-38), two levels deeper
//! than it lives. Cargo reads a relative `path` against the manifest it finds
//! it in, so `cargo add --path ../some-lib` writes a dependency that works for
//! the editor (the project workspace) and points somewhere else in the build.
//!
//! Every relative path is resolved where the author wrote it, against the
//! component's source directory. A target inside a tree that was copied whole
//! (a project's or package's `components/`) is the copy, which the relative
//! path still reaches when the component sits directly in that tree, so it is
//! left as written. Anything else becomes the absolute path it always meant.
//! Nothing checks whether a crate exists at either location: a path that
//! happened to reach SOME crate from the copy is precisely the bug.

use std::path::{Path, PathBuf};

/// The dependency tables of a manifest, at any level they can appear.
const DEP_TABLES: [&str; 5] =
    ["dependencies", "dev-dependencies", "dev_dependencies", "build-dependencies", "build_dependencies"];

/// Exempt by package name: a component's `trantor-abi = { path = "../../abi" }`
/// is meant to resolve in the GENERATED tree, beside which `abi` is emitted.
const GENERATED_ABI: &str = "trantor-abi";

/// A tree copied whole into the generated workspace: `from` is copied to `to`.
pub struct CopiedTree {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Where one component's manifest came from and where its copy is.
pub struct Placement<'a> {
    /// The component's source directory.
    pub origin: &'a Path,
    /// The directory holding the copied manifest.
    pub copy: &'a Path,
    pub trees: &'a [CopiedTree],
}

/// Rewrite `text`'s relative path dependencies for `placement`. Returns `None`
/// when nothing needed changing (or the manifest does not parse, which cargo
/// reports better), so the write-if-changed contract keeps holding.
pub fn anchor(text: &str, placement: &Placement) -> Option<String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let root = doc.as_table_mut();
    let mut changed = anchor_dep_tables(root, placement);
    for section in ["target", "patch"] {
        let Some(outer) = root.get_mut(section).and_then(|t| t.as_table_like_mut()) else {
            continue;
        };
        for (_, inner) in outer.iter_mut() {
            let Some(inner) = inner.as_table_like_mut() else { continue };
            changed |= if section == "target" {
                anchor_dep_tables(inner, placement)
            } else {
                anchor_deps(inner, placement)
            };
        }
    }
    changed.then(|| doc.to_string())
}

/// Anchor every dependency table directly under `table`.
fn anchor_dep_tables(table: &mut dyn toml_edit::TableLike, placement: &Placement) -> bool {
    let mut changed = false;
    for name in DEP_TABLES {
        if let Some(deps) = table.get_mut(name).and_then(|d| d.as_table_like_mut()) {
            changed |= anchor_deps(deps, placement);
        }
    }
    changed
}

/// Anchor each entry of one dependency table (`[patch.<source>]` included).
fn anchor_deps(deps: &mut dyn toml_edit::TableLike, placement: &Placement) -> bool {
    let mut changed = false;
    for (key, item) in deps.iter_mut() {
        let package = item.get("package").and_then(|v| v.as_str()).unwrap_or(key.get());
        if package == GENERATED_ABI {
            continue;
        }
        let Some(written) = item.get("path").and_then(|v| v.as_str()) else { continue };
        let Some(anchored) = anchored_path(written, placement) else { continue };
        item["path"] = toml_edit::value(anchored.to_string_lossy().into_owned());
        changed = true;
    }
    changed
}

/// The absolute path `written` must become, or `None` when it already reaches
/// the right place from the copy.
fn anchored_path(written: &str, placement: &Placement) -> Option<PathBuf> {
    if Path::new(written).is_absolute() {
        return None;
    }
    let meant = lexical_join(placement.origin, written);
    let wanted = placement
        .trees
        .iter()
        .find_map(|t| meant.strip_prefix(&t.from).ok().map(|rest| t.to.join(rest)))
        .unwrap_or(meant);
    (lexical_join(placement.copy, written) != wanted).then_some(wanted)
}

/// Join and normalize `..` LEXICALLY, the way cargo reads a path dependency.
///
/// Not `canonicalize`: that resolves symlinks first, and on macOS the temp and
/// var roots are symlinks (`/var` -> `/private/var`), which inserts a component
/// and makes the same `..` count land somewhere else entirely. Cargo never
/// looks at the filesystem to read these, so neither can this — measured, after
/// a physical resolution silently skipped every rewrite under $TMPDIR while
/// working perfectly under /private/tmp.
fn lexical_join(base: &Path, rel: &str) -> PathBuf {
    let mut out: Vec<std::ffi::OsString> = base.components().map(|c| c.as_os_str().to_os_string()).collect();
    for c in Path::new(rel).components() {
        match c {
            std::path::Component::ParentDir => {
                if out.len() > 1 {
                    out.pop();
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str().to_os_string()),
        }
    }
    out.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "/proj/components/upper-host";
    const COPY: &str = "/proj/target/trantor/w/components/upper-host";

    fn run(manifest: &str) -> Option<String> {
        let trees = [CopiedTree { from: "/proj/components".into(), to: "/proj/target/trantor/w/components".into() }];
        anchor(manifest, &Placement { origin: Path::new(ORIGIN), copy: Path::new(COPY), trees: &trees })
    }

    #[test]
    fn should_make_a_path_outside_the_project_absolute() {
        let out = run("[dependencies]\nupper-lib = { path = \"../../../upper-lib\" }\n").unwrap();
        assert!(out.contains("path = \"/upper-lib\""), "{out}");
    }

    #[test]
    fn should_anchor_a_path_that_stays_inside_the_project() {
        let out = run("[dependencies]\nlib = { path = \"../../lib\" }\n").unwrap();
        assert!(out.contains("path = \"/proj/lib\""), "{out}");
    }

    #[test]
    fn should_leave_a_sibling_in_the_copied_tree_alone() {
        assert_eq!(run("[dependencies]\ncore = { path = \"../sync-io-core\" }\n"), None);
    }

    #[test]
    fn should_leave_an_absolute_path_alone() {
        assert_eq!(run("[dependencies]\nlib = { path = \"/elsewhere/lib\" }\n"), None);
    }

    #[test]
    fn should_leave_the_generated_abi_alone() {
        assert_eq!(run("[dependencies]\ntrantor-abi = { path = \"../../abi\" }\n"), None);
    }

    #[test]
    fn should_leave_a_renamed_generated_abi_alone() {
        assert_eq!(run("[dependencies]\nabi = { package = \"trantor-abi\", path = \"../../abi\" }\n"), None);
    }

    #[test]
    fn should_anchor_dev_and_build_dependencies() {
        let out = run("[dev-dependencies]\na = { path = \"../../a\" }\n[build-dependencies]\nb = { path = \"../../b\" }\n")
            .unwrap();
        assert!(out.contains("\"/proj/a\"") && out.contains("\"/proj/b\""), "{out}");
    }

    #[test]
    fn should_anchor_target_specific_dependencies() {
        let out = run("[target.'cfg(unix)'.dependencies]\na = { path = \"../../a\" }\n").unwrap();
        assert!(out.contains("\"/proj/a\""), "{out}");
    }

    #[test]
    fn should_anchor_dotted_target_tables() {
        let out = run("[target.x86_64-apple-darwin.build-dependencies]\na = { path = \"../../a\" }\n").unwrap();
        assert!(out.contains("\"/proj/a\""), "{out}");
    }

    #[test]
    fn should_anchor_patch_entries() {
        let out = run("[patch.crates-io]\nserde = { path = \"../../serde\" }\n").unwrap();
        assert!(out.contains("\"/proj/serde\""), "{out}");
    }

    #[test]
    fn should_anchor_a_standard_table_dependency() {
        let out = run("[dependencies.lib]\npath = \"../../lib\"\nversion = \"0.1\"\n").unwrap();
        assert!(out.contains("path = \"/proj/lib\""), "{out}");
    }

    #[test]
    fn should_keep_the_rest_of_the_manifest() {
        let manifest = "# mine\n[package]\nname = \"x\"\n[dependencies]\nlib = { version = \"0.1\", path = \"../../lib\" }\n";
        let out = run(manifest).unwrap();
        assert!(out.starts_with("# mine\n[package]\nname = \"x\"\n") && out.contains("version = \"0.1\""), "{out}");
    }

    #[test]
    fn should_leave_a_manifest_with_no_path_deps_unchanged() {
        assert_eq!(run("[dependencies]\nserde = \"1\"\n"), None);
    }

    #[test]
    fn should_map_an_in_tree_target_to_its_copy_when_the_component_lives_deeper() {
        let trees = [CopiedTree { from: "/proj/components".into(), to: "/proj/target/trantor/w/components".into() }];
        let placement = Placement {
            origin: Path::new("/proj/components/group/leaf"),
            copy: Path::new("/proj/target/trantor/w/components/leaf"),
            trees: &trees,
        };
        let out = anchor("[dependencies]\ncore = { path = \"../core\" }\n", &placement).unwrap();
        assert!(out.contains("\"/proj/target/trantor/w/components/group/core\""), "{out}");
    }
}
