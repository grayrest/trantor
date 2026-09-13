//! What a composed platform requires of its app, read from its
//! `platform/main.roc`, and the app `trantor new` writes against it.
//!
//! The signature is the driver's to decide — trantor-cli's takes
//! `List(OsStr)`, a bare driver's takes `{}` — so a scaffold that writes one
//! from memory is wrong for every baseline but the one it remembered.

/// `main!`'s signature as the platform declares it, the modules its types come
/// from, and a parameter list and body that typecheck against it.
#[derive(Debug)]
pub struct MainContract {
    pub signature: String,
    pub imports: Vec<String>,
    /// `{}`, `_args`, or `_arg1, _arg2` — one per argument.
    pub param: String,
    /// `Ok({})` for a `Try({}, _)` return; otherwise a `crash` naming what to
    /// write, since no value of an arbitrary type can be produced from nothing.
    pub body: String,
}

pub fn main_contract(platform: &str) -> Result<MainContract, String> {
    let text: String = platform.lines().map(|l| l.split_once('#').map_or(l, |(code, _)| code)).collect::<Vec<_>>().join("\n");
    let block = requires_block(&text).ok_or("the composed platform has no `requires { ... }` block")?;
    let entries = entries(&block);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    let Some((_, ty)) = entries.iter().find(|(n, _)| n == "main!") else {
        return Err(format!("the composed platform requires no `main!` — it requires {{{}}} — so there is no app shape to scaffold", names.join(", ")));
    };
    if entries.len() > 1 {
        return Err(format!("the composed platform requires {} as well as `main!`, and the scaffold writes only `main!`", names.iter().filter(|n| **n != "main!").map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")));
    }
    let signature = format!("main! : {ty}");
    let exposed: Vec<String> = text
        .split_once("exposes")
        .and_then(|(_, r)| r.split_once('[').and_then(|(_, r)| r.split_once(']')))
        .map(|(inner, _)| inner.split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect())
        .unwrap_or_default();
    let mut imports: Vec<String> = ty
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && exposed.iter().any(|e| e == w))
        .map(|w| format!("pf.{w}"))
        .collect();
    imports.sort();
    imports.dedup();
    let (args, ret) = split_top_level(ty, "=>").or_else(|| split_top_level(ty, "->")).unwrap_or((ty, ""));
    let arity = if args.trim().is_empty() { 0 } else { top_level_commas(args) + 1 };
    let param = match arity {
        _ if args.trim() == "{}" => "{}".to_string(),
        0 | 1 => "_args".to_string(),
        n => (1..=n).map(|i| format!("_arg{i}")).collect::<Vec<_>>().join(", "),
    };
    let ok_is_unit = ret.trim().strip_prefix("Try(").and_then(|r| split_top_level(r, ",")).is_some_and(|(ok, _)| ok.trim() == "{}");
    let body = if ok_is_unit { "Ok({})".to_string() } else { "crash \"write your program here\"".to_string() };
    Ok(MainContract { signature, imports, param, body })
}

/// The inside of `requires { ... }`, comments already removed.
fn requires_block(text: &str) -> Option<String> {
    let after = text.split_once("requires")?.1;
    let open = after.find('{')?;
    let mut depth = 0;
    for (i, c) in after[open..].char_indices() {
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some(after[open + 1..open + i].to_string());
        }
    }
    None
}

/// `name : type` entries, split where a top-level `name :` starts — after a
/// comma or on a new line; each type's whitespace collapsed.
fn entries(block: &str) -> Vec<(String, String)> {
    let mut chunks: Vec<String> = vec![String::new()];
    let mut depth = 0;
    for line in block.lines() {
        if depth == 0 && starts_entry(line) && !chunks.last().is_some_and(|c| c.trim().is_empty()) {
            chunks.push(String::new());
        }
        for (i, c) in line.char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
            // A comma between a function's arguments is not a new entry; one
            // followed by `name :` is.
            if depth == 0 && c == ',' && starts_entry(&line[i + 1..]) {
                chunks.push(String::new());
            } else {
                chunks.last_mut().expect("never empty").push(c);
            }
        }
        chunks.last_mut().expect("never empty").push(' ');
    }
    chunks
        .iter()
        .filter_map(|c| c.split_once(':').map(|(n, t)| (n.trim().to_string(), t.split_whitespace().collect::<Vec<_>>().join(" "))))
        .filter(|(n, _)| !n.is_empty())
        .collect()
}

fn starts_entry(line: &str) -> bool {
    line.split_once(':').is_some_and(|(n, rest)| {
        let n = n.trim();
        !rest.starts_with(':') && !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '!')
    })
}

fn split_top_level<'a>(s: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c { '(' | '[' | '{' => depth += 1, ')' | ']' | '}' => depth -= 1, _ => {} }
        if depth == 0 && s[i..].starts_with(sep) {
            return Some((&s[..i], &s[i + sep.len()..]));
        }
    }
    None
}

fn top_level_commas(s: &str) -> usize {
    let mut depth = 0;
    s.chars().filter(|&c| {
        match c { '(' | '[' | '{' => depth += 1, ')' | ']' | '}' => depth -= 1, _ => {} }
        depth == 0 && c == ','
    }).count()
}

pub fn app_main(name: &str, contract: &MainContract) -> String {
    format!(
        "app [main!] {{ pf: platform \"../target/trantor/{name}/platform/main.roc\" }}\n\n{}\n\n{}\nmain! = |{}| {{\n\t{}\n}}\n",
        contract.imports.iter().map(|i| format!("import {i}")).collect::<Vec<_>>().join("\n"),
        contract.signature,
        contract.param,
        contract.body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLI: &str = "platform \"\"\n\trequires {\n\t\tmain! : List(OsStr) => Try({}, [Io(IOErr), ..])\n\t}\n\texposes [Cli, IOErr, OsStr, Stdout]\n";
    const BARE: &str = "platform \"\"\n\trequires {\n\t\tmain! : {} => Try({}, [Exit(I32), ..])\n\t}\n\texposes [Stdout]\n";

    fn contract(requires: &str) -> Result<MainContract, String> {
        main_contract(&format!("platform \"\"\n\trequires {{\n{requires}\n\t}}\n\texposes [IOErr, OsStr]\n"))
    }

    #[test]
    fn a_wrapped_repeated_or_non_try_signature_still_scaffolds_an_app_that_can_typecheck() {
        let wrapped = contract("\t\tmain! : List(OsStr)\n\t\t\t=> Try({}, [Io(IOErr), BadArg(OsStr)])").unwrap();
        assert_eq!(wrapped.signature, "main! : List(OsStr) => Try({}, [Io(IOErr), BadArg(OsStr)])");
        assert_eq!(wrapped.imports, vec!["pf.IOErr", "pf.OsStr"], "a type named twice is imported once");
        let int = contract("\t\tmain! : {} => I32").unwrap();
        assert_eq!((int.param.as_str(), int.body.starts_with("crash")), ("{}", true));
        let two = contract("\t\tmain! : Str, List(Str) => Try({}, _)").unwrap();
        assert_eq!(two.param, "_arg1, _arg2");
        assert!(contract("\t\trun! : {} => {}").unwrap_err().contains("requires no `main!`"));
    }

    #[test]
    fn only_a_try_whose_ok_is_unit_gets_ok_unit() {
        assert_eq!(contract("\t\tmain! : {} => Try({}, [Exit(I32), ..])").unwrap().body, "Ok({})");
        assert!(contract("\t\tmain! : {} => Try(I32, [Exit(I32), ..])").unwrap().body.starts_with("crash"));
    }

    #[test]
    fn a_record_argument_wrapped_over_lines_is_one_argument() {
        let c = contract("\t\tmain! : {\n\t\t\tname : Str,\n\t\t\tage : U8,\n\t\t} => Try({}, _)").unwrap();
        assert_eq!(c.signature, "main! : { name : Str, age : U8, } => Try({}, _)");
        assert_eq!(c.param, "_args");
    }

    #[test]
    fn comments_in_or_around_the_requires_block_are_not_the_signature() {
        let c = contract("\t\t# main! : {} => I32 was the old contract\n\t\tmain! : List(OsStr) # the args\n\t\t\t=> Try({}, _)").unwrap();
        assert_eq!(c.signature, "main! : List(OsStr) => Try({}, _)");
    }

    #[test]
    fn another_required_entry_is_refused_rather_than_left_undefined() {
        for requires in ["\t\tmain! : {} => Try({}, _), tag : Str", "\t\tmain! : {} => Try({}, _)\n\t\ttag : Str"] {
            let e = contract(requires).unwrap_err();
            assert!(e.contains("`tag`"), "{e}");
        }
    }

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
}
