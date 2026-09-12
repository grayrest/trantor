//! `trantor interface-stub` — the Rust signature for a hosted interface's
//! leaves, generated instead of guessed (D-U1-7, plan U1 P1).
//!
//! Writing a host component means writing `#[unsafe(no_mangle)] extern
//! "C-unwind" fn trantor__<component>__<stem>` with the argument type roc's
//! glue chose. That name is `SubprocessHostExecOutputArgs`, and nothing tells
//! you so: it is learnable only by building and reading `abi/src/generated.rs`.
//! Every new interface therefore began with a guess-compile-read-fix loop, and
//! this is the loop's replacement — the nearest thing trantor has to the
//! compiler telling you the signature you were supposed to write.
//!
//! Two sources, each for what it actually knows. The interface's own `.roc`
//! module gives the Roc declaration (the leaf's type, verbatim, as a doc
//! comment). The generated glue gives the Rust type names, because those are
//! glue's to choose and cannot be re-derived here without reimplementing it.

use crate::manifest::{self, World};
use std::path::{Path, PathBuf};

/// `exec_output` -> `ExecOutput`. Glue's own leaf-to-type-name convention.
fn pascal(stem: &str) -> String {
    stem.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Does the glue define this type? Matches the `pub struct`/`pub union`/`pub
/// enum` definition rather than any mention, so a type named only inside
/// another type's body is not mistaken for one that exists.
fn defines(glue: &str, ty: &str) -> bool {
    ["pub struct ", "pub union ", "pub enum "].iter().any(|kw| {
        glue.lines().any(|l| {
            l.trim_start().strip_prefix(kw).is_some_and(|rest| {
                rest.starts_with(ty)
                    && !rest[ty.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            })
        })
    })
}

/// Whether the glue emits a whole-struct `decref` for this type. It does so
/// only for MULTI-field structs; a single-field `TextShoutArgs { arg0: RocStr }`
/// gets none (measured), so this is necessary but not sufficient to decide
/// whether an argument must be released — see `release_plan`.
///
/// The doc line "Refcounted fields are owned by the hosted function" is printed
/// on every args struct including all-scalar ones (measured on
/// `ClocksSleepMillisArgs`), so it is not a signal at all.
fn has_decref(glue: &str, ty: &str) -> bool {
    let Some(at) = glue.find(&format!("\nimpl {ty} {{")) else {
        return false;
    };
    let body = &glue[at + 1..];
    let end = body.find("\n}\n").map_or(body.len(), |e| e);
    body[..end].contains("fn decref")
}

/// The `(name, type)` fields of a glue struct. Glue emits each struct twice,
/// once per pointer width, with identical fields and differing layout asserts;
/// the first is enough.
fn fields_of(glue: &str, ty: &str) -> Vec<(String, String)> {
    let Some(at) = glue.find(&format!("\npub struct {ty} {{")) else {
        return Vec::new();
    };
    let body = &glue[at..];
    let start = body.find('{').map_or(0, |i| i + 1);
    let end = body.find("\n}").unwrap_or(body.len());
    if end <= start {
        return Vec::new();
    }
    body[start..end]
        .lines()
        .filter_map(|l| {
            let l = l.trim().trim_end_matches(',');
            let rest = l.strip_prefix("pub ")?;
            let (name, ty) = rest.split_once(':')?;
            Some((name.trim().to_string(), ty.trim().to_string()))
        })
        .collect()
}

/// Does a value of this type own a Roc reference?
fn is_refcounted(glue: &str, ty: &str) -> bool {
    let t = ty.trim();
    t == "RocStr"
        || t.starts_with("RocList")
        || t.starts_with("RocBox")
        || t.starts_with("RocResource")
        || has_decref(glue, t)
}

/// How the hosted function must release its owned argument (B0). Glue's
/// whole-struct `decref` when it emitted one, else one call per refcounted
/// field, else nothing — and "nothing" has to be earned, because a missed
/// release is a leak the type system will not mention.
fn release_plan(glue: &str, args_ty: &str, binding: &str) -> Vec<String> {
    if has_decref(glue, args_ty) {
        return vec![format!("unsafe {{ {binding}.decref(abi::host()); }}")];
    }
    fields_of(glue, args_ty)
        .into_iter()
        .filter(|(_, t)| is_refcounted(glue, t))
        .map(|(f, _)| format!("unsafe {{ {binding}.{f}.decref(abi::host()); }}"))
        .collect()
}

/// Map a Roc type to its Rust spelling for the cases glue does not name a type
/// for (a leaf returning a primitive gets no `…Result` struct). Anything
/// structured returns `None` and the stub says so out loud rather than
/// inventing a name that will not exist.
fn rust_type(roc: &str) -> Option<String> {
    let t = roc.trim();
    let prim = |s: &str| -> Option<String> {
        Some(
            match s {
                "Str" => "RocStr",
                "Bool" => "bool",
                "U8" => "u8",
                "U16" => "u16",
                "U32" => "u32",
                "U64" => "u64",
                "U128" => "u128",
                "I8" => "i8",
                "I16" => "i16",
                "I32" => "i32",
                "I64" => "i64",
                "I128" => "i128",
                "F32" => "f32",
                "F64" => "f64",
                _ => return None,
            }
            .to_string(),
        )
    };
    if t == "{}" {
        return Some(String::new()); // unit: no `->` clause at all
    }
    if let Some(p) = prim(t) {
        return Some(p);
    }
    if let Some(inner) = t.strip_prefix("List(").and_then(|r| r.strip_suffix(')')) {
        return rust_type(inner).filter(|s| !s.is_empty()).map(|i| format!("RocList<{i}>"));
    }
    // A bare nominal (`Fs.Descriptor`, `IOErr`) is emitted by glue under its
    // last segment.
    let last = t.rsplit('.').next().unwrap_or(t);
    if !last.is_empty()
        && last.starts_with(|c: char| c.is_ascii_uppercase())
        && last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Some(last.to_string());
    }
    None
}

/// The leaf's declaration as written in the interface's Roc module, e.g.
/// `exec_output! : Cmd => Try(…)`. Read from the shipped module because that is
/// the authored source of truth; the glue's copy is a paraphrase.
fn roc_decl(module_src: &str, leaf: &str) -> Option<String> {
    module_src.lines().find_map(|l| {
        let t = l.trim();
        let rest = t.strip_prefix(leaf)?;
        let rest = rest.trim_start();
        rest.strip_prefix(':').map(|sig| sig.trim().to_string())
    })
}

/// The argument side of `args => ret`, at depth zero.
fn args_of(sig: &str) -> Option<String> {
    let b = sig.as_bytes();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' | b'-' if depth == 0 && i + 1 < b.len() && b[i + 1] == b'>' => {
                return Some(sig[..i].trim().to_string());
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The return side of `args => ret` / `args -> ret`, at depth zero so an arrow
/// inside a nested type is not mistaken for the top-level one.
fn return_of(sig: &str) -> Option<String> {
    let b = sig.as_bytes();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' | b'-' if depth == 0 && i + 1 < b.len() && b[i + 1] == b'>' => {
                return Some(sig[i + 2..].trim().to_string());
            }
            _ => {}
        }
        i += 1;
    }
    None
}

pub fn interface_stub(dir: &Path, world_file: &str, iface_name: &str) -> Result<(), String> {
    let world: World = manifest::load_world(dir, world_file)?;
    let iface = manifest::load_interface(dir, &world, iface_name)?;

    if iface.is_service() {
        return Err(format!(
            "interface `{iface_name}` is a service (kind = \"service\"), not a hosted interface. \
             A service implements the five contract symbols — init, cmd, complete, gate, and env \
             when it owns one — described in abi/src/services.rs, not per-leaf hosted functions."
        ));
    }
    if iface.hosted.is_empty() {
        return Err(format!(
            "interface `{iface_name}` declares no [[hosted]] leaves, so there is nothing to \
             implement. A type-only interface (io's IOErr) ships a module and no host code."
        ));
    }

    // Which component implements it, and therefore how the symbols are mangled.
    let wiring = world.wiring.get(iface_name).ok_or_else(|| {
        format!(
            "interface `{iface_name}` is not wired in this world, so no component owns its \
             symbols. Add `{iface_name} = \"<component>\"` under [wiring]."
        )
    })?;
    let head = wiring.split('(').next().unwrap_or(wiring).trim();
    let prefix = format!("trantor__{}__", crate::resolve::sanitize(head));

    // The glue, which is the only place the argument type names exist.
    let gen: PathBuf = manifest::out_dir(dir, &world).join("abi/src/generated.rs");
    let glue = std::fs::read_to_string(&gen).map_err(|e| {
        format!(
            "read {}: {e}\n\
             \n\
             The glue does not exist yet. Run `trantor build {} --platform-only` once first: it \
             composes and glues before it needs any host code, so the glue is on disk \
             afterwards even when the component has none.",
            gen.display(),
            dir.display()
        )
    })?;
    if glue.trim().is_empty() {
        return Err(format!("{} is empty — re-run the glue step", gen.display()));
    }

    // The interface's Roc module, for the authored declarations.
    let module_file =
        manifest::iface_dir(dir, &world, iface_name).join(format!("{}.roc", iface.module));
    let module_src = std::fs::read_to_string(&module_file).unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        "//! Host implementation of interface `{iface_name}` (module `{}`), wired to `{head}`.\n\
         //! Signatures generated by `trantor interface-stub`; bodies are yours.\n\
         use trantor_abi as abi;\n\
         use abi::*;\n\n",
        iface.module
    ));

    // Glue emits ONE result type per distinct Roc return and names it after the
    // first leaf that used it, so `exec_status : Cmd => Try(I32, IOErr)` is
    // served by `SubprocessHostExecExitCodeResult` and no
    // `…ExecStatusResult` exists. Index the ones that do exist by their return
    // so the shared name is found rather than reported missing.
    let mut result_of: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for leaf in &iface.hosted {
        let ty = format!("{}{}Result", iface.module, pascal(&leaf.symbol_stem));
        if !defines(&glue, &ty) {
            continue;
        }
        if let Some(ret) = roc_decl(&module_src, &leaf.leaf).as_deref().and_then(return_of) {
            result_of.entry(ret).or_insert(ty);
        }
    }

    // The glue must already know this interface. If a leaf TAKES arguments and
    // glue defines no args struct for it, the glue predates the interface —
    // and without this check the stub silently emits a zero-argument function,
    // which compiles, links against nothing, and is wrong in the one way the
    // whole command exists to prevent.
    let stale: Vec<&str> = iface
        .hosted
        .iter()
        .filter(|l| {
            let takes_args = roc_decl(&module_src, &l.leaf)
                .as_deref()
                .and_then(args_of)
                .is_some_and(|a| a != "{}");
            takes_args && !defines(&glue, &format!("{}{}Args", iface.module, pascal(&l.symbol_stem)))
        })
        .map(|l| l.leaf.as_str())
        .collect();
    if !stale.is_empty() {
        return Err(format!(
            "the generated glue does not describe `{iface_name}` yet: {stale:?} take arguments \
             and no `{}<Leaf>Args` type exists in {}.\n\n\
             Run `trantor build {} --platform-only` to regenerate it, then re-run this. \
             (Generating anyway would emit a zero-argument function: it compiles, and it is \
             wrong.)",
            iface.module,
            gen.display(),
            dir.display()
        ));
    }

    let mut unresolved = 0usize;
    for leaf in &iface.hosted {
        let p = pascal(&leaf.symbol_stem);
        let args_ty = format!("{}{p}Args", iface.module);
        let result_ty = format!("{}{p}Result", iface.module);
        let decl = roc_decl(&module_src, &leaf.leaf);

        if let Some(d) = &decl {
            out.push_str(&format!("/// Roc: `{}.{} : {d}`\n", iface.module, leaf.leaf));
        }

        // Argument: glue's struct when it made one (it does for every non-unit
        // argument, single scalars included — `CellPutArgs { arg0: RocStr }`).
        let (param, arg_expr) = if defines(&glue, &args_ty) {
            ("a: ".to_string() + &args_ty, Some("a"))
        } else {
            (String::new(), None)
        };

        // Return: glue's struct when it made one, else the primitive mapping.
        let roc_ret = decl.as_deref().and_then(return_of);
        let ret = if defines(&glue, &result_ty) {
            format!(" -> {result_ty}")
        } else if let Some(shared) = roc_ret.as_deref().and_then(|r| result_of.get(r)) {
            format!(" -> {shared}")
        } else {
            match roc_ret.as_deref().map(rust_type) {
                Some(Some(t)) if t.is_empty() => String::new(),
                Some(Some(t)) => format!(" -> {t}"),
                _ => {
                    unresolved += 1;
                    out.push_str(
                        "// FIXME(trantor): the glue names no type for this return and it is not \
                         a primitive.\n// Find it in the generated glue and write it in.\n",
                    );
                    String::new()
                }
            }
        };

        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!(
            "pub extern \"C-unwind\" fn {prefix}{}({param}){ret} {{\n",
            leaf.symbol_stem
        ));
        // The owned-argument rule (B0): a hosted call owns its arguments and
        // must release them. Emitted only where glue proves there is something
        // refcounted to release.
        if let Some(a) = arg_expr {
            let plan = release_plan(&glue, &args_ty, a);
            if plan.is_empty() {
                out.push_str(&format!(
                    "    let _ = {a}; // no refcounted field: nothing to release\n"
                ));
            } else {
                out.push_str(
                    "    // Owned argument (B0): released once, after the last read of it.\n",
                );
                for call in plan {
                    out.push_str(&format!("    {call}\n"));
                }
            }
        }
        out.push_str("    todo!()\n}\n\n");
    }

    let target = manifest::component_dir(
        dir,
        head,
        world
            .components
            .get(head)
            .ok_or_else(|| format!("wired component `{head}` is not declared in this world"))?,
    )
    .join("src/lib.rs");

    // Refuse to clobber real work; replace a placeholder without ceremony.
    // "Real work" is a hosted function — a scaffolded lib.rs is a comment and
    // a manifest that would not otherwise parse.
    let has_impl = std::fs::read_to_string(&target)
        .is_ok_and(|t| t.contains("extern \"C-unwind\""));
    if has_impl {
        print!("{out}");
        eprintln!(
            "trantor: {} leaf/leaves for `{iface_name}` -> stdout ({} already exists; \
             re-run after changing the Roc declaration and paste what moved)",
            iface.hosted.len(),
            target.display()
        );
    } else {
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("create {}: {e}", p.display()))?;
        }
        std::fs::write(&target, &out).map_err(|e| format!("write {}: {e}", target.display()))?;
        eprintln!(
            "trantor: wrote {} ({} leaf/leaves for `{iface_name}`)",
            target.display(),
            iface.hosted.len()
        );
    }
    if unresolved > 0 {
        return Err(format!(
            "{unresolved} of {} leaves have a return type the stub could not name — see the \
             FIXME lines",
            iface.hosted.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_joins_stem_words() {
        assert_eq!(pascal("exec_output"), "ExecOutput");
        assert_eq!(pascal("ping"), "Ping");
        assert_eq!(pascal("read_to_end"), "ReadToEnd");
    }

    #[test]
    fn defines_matches_only_whole_type_names() {
        let g = "pub struct FsOpenAtArgs {\n    pub arg0: u64,\n}\n";
        assert!(defines(g, "FsOpenAtArgs"));
        // A prefix of a defined name is not itself defined.
        assert!(!defines(g, "FsOpenAt"));
        assert!(!defines(g, "FsOpenAtArgsRelease"));
    }

    #[test]
    fn fields_of_reads_a_glue_struct() {
        let g = "\npub struct TextShoutArgs {\n    pub arg0: RocStr,\n}\n";
        assert_eq!(fields_of(g, "TextShoutArgs"), vec![("arg0".into(), "RocStr".into())]);
    }

    #[test]
    fn a_single_refcounted_field_is_released_even_with_no_struct_decref() {
        // Glue emits a whole-struct decref only for MULTI-field structs, so the
        // single-Str case must still be released — per field. Getting this
        // wrong emits "nothing to release" over a leak.
        let g = "\npub struct TextShoutArgs {\n    pub arg0: RocStr,\n}\n";
        assert_eq!(
            release_plan(g, "TextShoutArgs", "a"),
            vec!["unsafe { a.arg0.decref(abi::host()); }".to_string()]
        );
    }

    #[test]
    fn a_whole_struct_decref_wins_over_per_field() {
        let g = "\npub struct M {\n    pub x: RocStr,\n    pub y: RocStr,\n}\n\nimpl M {\n    pub unsafe fn decref(self) {}\n}\n";
        assert_eq!(release_plan(g, "M", "a"), vec!["unsafe { a.decref(abi::host()); }".to_string()]);
    }

    #[test]
    fn an_all_scalar_struct_releases_nothing() {
        let g = "\npub struct ClocksSleepMillisArgs {\n    pub arg0: u64,\n}\n";
        assert!(release_plan(g, "ClocksSleepMillisArgs", "a").is_empty());
    }

    #[test]
    fn has_decref_is_the_impl_not_the_doc_line() {
        let with = "\nimpl A {\n    pub unsafe fn decref(self) {}\n}\n";
        let without = "\nimpl B {\n    pub fn other(self) {}\n}\n";
        assert!(has_decref(with, "A"));
        assert!(!has_decref(without, "B"));
    }

    #[test]
    fn args_of_splits_at_the_top_level_arrow() {
        assert_eq!(args_of("Str => Str").unwrap(), "Str");
        assert_eq!(args_of("{} => Str").unwrap(), "{}", "a unit argument is not an argument");
        assert_eq!(args_of("{ a : Str, b : U64 } => {}").unwrap(), "{ a : Str, b : U64 }");
    }

    #[test]
    fn return_of_ignores_arrows_inside_nested_types() {
        assert_eq!(return_of("Cmd => Try(A, B)").unwrap(), "Try(A, B)");
        assert_eq!(return_of("{} => Str").unwrap(), "Str");
        // The arrow inside the record must not win.
        assert_eq!(return_of("{ f : A => B } => Str").unwrap(), "Str");
        assert_eq!(return_of("Str"), None);
    }

    #[test]
    fn rust_type_maps_primitives_and_lists_and_refuses_structures() {
        assert_eq!(rust_type("Str").unwrap(), "RocStr");
        assert_eq!(rust_type("U64").unwrap(), "u64");
        assert_eq!(rust_type("{}").unwrap(), "");
        assert_eq!(rust_type("List(U8)").unwrap(), "RocList<u8>");
        assert_eq!(rust_type("Fs.Descriptor").unwrap(), "Descriptor");
        assert_eq!(rust_type("Try(A, B)"), None);
        assert_eq!(rust_type("{ a : Str }"), None);
    }

    #[test]
    fn shared_result_types_are_found_by_identical_return() {
        // Glue names one result type per distinct return, after the FIRST leaf
        // using it; a later leaf with the same return gets no type of its own.
        let mut by_ret = std::collections::BTreeMap::new();
        by_ret.insert("Try(I32, IOErr)".to_string(), "SubprocessHostExecExitCodeResult".to_string());
        assert_eq!(
            by_ret.get(&return_of("Cmd => Try(I32, IOErr)").unwrap()).unwrap(),
            "SubprocessHostExecExitCodeResult"
        );
        assert!(!by_ret.contains_key(&return_of("Cmd => Try(A, B)").unwrap()));
    }

    #[test]
    fn roc_decl_finds_the_leaf_line() {
        let src = "Cell :: [].{\n\tput! : Str => {}\n\tget! : {} => Str\n}\n";
        assert_eq!(roc_decl(src, "put!").unwrap(), "Str => {}");
        assert_eq!(roc_decl(src, "get!").unwrap(), "{} => Str");
        assert_eq!(roc_decl(src, "nope!"), None);
    }
}
