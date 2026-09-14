//! Splice markers in driver-owned Roc modules (D-H7-6). The driver keeps
//! authoring `Cmd.roc` / `Event.roc` / `Env.roc` — its boundary docs and core
//! variants stay put — and marks one block in each:
//!
//! ```roc
//! Cmd := [
//!     Log(Str),
//!     ## @trantor(cmd)
//!     ## @end
//! ]
//! ```
//!
//! trantor replaces the lines between the markers with one wrapper per
//! service component (`Notes(Notes),` / `Notes(NotesEvent),` /
//! `notes : NotesEnv,`) and adds the `import`s those wrappers need. A module
//! without markers is copied verbatim. This extends D18-C ("the driver's
//! contract text is spliced") and does not breach D13: the marker is the driver
//! declaring a splice point in its own contract.
//!
//! Also here: a spliced union must be a tag union. A single-variant one is
//! allowed; glue unwraps it and glue_unions.rs repairs the output (D-H7-44).

use crate::resolve::Service;

const MARKER_PREFIX: &str = "## @trantor(";
const MARKER_END: &str = "## @end";

/// Which block a marker names.
#[derive(Clone, Copy, PartialEq)]
enum Block {
    Cmd,
    Event,
    Env,
}

fn parse_marker(line: &str) -> Option<Block> {
    let inner = line.trim().strip_prefix(MARKER_PREFIX)?.strip_suffix(')')?;
    match inner {
        "cmd" => Some(Block::Cmd),
        "event" => Some(Block::Event),
        "env" => Some(Block::Env),
        _ => None,
    }
}

/// `NotesEvent` -> `notes_event`: glue's field spelling for a wrapper variant,
/// and the Env field name for an env block.
pub fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// The lines a block splices in, and the modules they reference.
fn block_lines(block: Block, services: &[Service]) -> (Vec<String>, Vec<String>) {
    let mut lines = Vec::new();
    let mut modules = Vec::new();
    for s in services {
        match block {
            Block::Cmd => {
                lines.push(format!("{}({}),", s.module, s.module));
                modules.push(s.module.clone());
            }
            Block::Event => {
                if let Some(ev) = &s.event_module {
                    lines.push(format!("{}({ev}),", s.module));
                    modules.push(ev.clone());
                }
            }
            Block::Env => {
                if let Some(env) = &s.env_module {
                    lines.push(format!("{} : {env},", snake_case(&s.module)));
                    modules.push(env.clone());
                }
            }
        }
    }
    (lines, modules)
}

/// Fill every marked block in `text` from `services`. Errors on an unterminated
/// marker. Returns the text unchanged when it has no markers.
pub fn splice(text: &str, services: &[Service]) -> Result<String, String> {
    if !text.contains(MARKER_PREFIX) {
        return Ok(text.to_string());
    }
    let mut out: Vec<String> = Vec::new();
    let mut needed: Vec<String> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(block) = parse_marker(line) else {
            out.push(line.to_string());
            continue;
        };
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        out.push(line.to_string());
        let (body, modules) = block_lines(block, services);
        for b in body {
            out.push(format!("{indent}{b}"));
        }
        needed.extend(modules);
        // Skip the old body up to and including the end marker.
        let mut closed = false;
        for inner in lines.by_ref() {
            if inner.trim() == MARKER_END {
                out.push(inner.to_string());
                closed = true;
                break;
            }
        }
        if !closed {
            return Err(format!("splice: `{}` has no closing `{MARKER_END}`", line.trim()));
        }
    }
    let mut result = insert_imports(out, &needed);
    if text.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

/// Add `import M` for every module in `needed` the file does not already
/// import, before its first `import` line or, failing that, after the leading
/// comment block.
fn insert_imports(mut lines: Vec<String>, needed: &[String]) -> String {
    let mut to_add: Vec<String> = Vec::new();
    for m in needed {
        let stmt = format!("import {m}");
        let present = lines.iter().any(|l| l.trim() == stmt || l.trim().starts_with(&format!("{stmt} ")));
        if !present && !to_add.contains(&stmt) {
            to_add.push(stmt);
        }
    }
    if to_add.is_empty() {
        return lines.join("\n");
    }
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with("import "))
        .unwrap_or_else(|| {
            lines
                .iter()
                .position(|l| !(l.trim().is_empty() || l.trim_start().starts_with("##")))
                .unwrap_or(lines.len())
        });
    for (i, stmt) in to_add.into_iter().enumerate() {
        lines.insert(at + i, stmt);
    }
    lines.join("\n")
}

/// Field counts per variant of the top-level union `name := [ … ]` in a Roc
/// module, or `None` when the declaration is not a union (a record, say).
/// Nested parens/brackets inside a payload are balanced, not parsed.
pub fn union_variants(text: &str, name: &str) -> Option<Vec<usize>> {
    let start = text.find(&format!("{name} :="))?;
    let after = text[start..].find('[')? + start + 1;
    let mut depth = 0i32; // bracket/paren depth inside the union body
    let mut variants = Vec::new();
    let mut fields = 0usize;
    let mut in_variant = false;
    let mut in_comment = false;
    for c in text[after..].chars() {
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            continue;
        }
        match c {
            '#' => in_comment = true,
            '(' | '[' | '{' => {
                if depth == 0 && c == '(' {
                    fields = 1;
                }
                depth += 1;
            }
            ')' | '}' => depth -= 1,
            ']' => {
                if depth == 0 {
                    if in_variant {
                        variants.push(fields);
                    }
                    return Some(variants);
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                if in_variant {
                    variants.push(fields);
                }
                in_variant = false;
                fields = 0;
            }
            ',' if depth == 1 => fields += 1,
            c if c.is_ascii_uppercase() && depth == 0 && !in_variant => {
                in_variant = true;
                fields = 0;
            }
            _ => {}
        }
    }
    None
}

/// A spliced union must be a tag union with at least one variant. One variant
/// is fine: glue unwraps it, and composition repairs the output (D-H7-44,
/// glue_unions.rs) instead of the service growing a second command (D-H7-21).
pub fn check_spliceable(text: &str, name: &str) -> Result<(), String> {
    let Some(variants) = union_variants(text, name) else {
        return Err(format!("`{name}` is not a tag union (`{name} := [...]`)"));
    };
    if variants.is_empty() {
        return Err(format!("`{name}` has no variants"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(module: &str, event: Option<&str>, env: Option<&str>) -> Service {
        Service {
            component: format!("svc-{}", snake_case(module)),
            module: module.to_string(),
            event_module: event.map(str::to_string),
            env_module: env.map(str::to_string),
        }
    }

    #[test]
    fn splices_cmd_block_and_imports() {
        let text = "## docs\n\nCmd := [\n\tLog(Str),\n\t## @trantor(cmd)\n\tStale(U64),\n\t## @end\n]\n";
        let out = splice(text, &[svc("Notes", Some("NotesEvent"), None), svc("Dbx", None, None)]).unwrap();
        assert_eq!(
            out,
            "## docs\n\nimport Notes\nimport Dbx\nCmd := [\n\tLog(Str),\n\t## @trantor(cmd)\n\tNotes(Notes),\n\tDbx(Dbx),\n\t## @end\n]\n"
        );
    }

    #[test]
    fn splices_event_and_env_blocks_only_for_services_that_have_them() {
        let text = "import Id\n\nEvent := [\n    Click,\n    ## @trantor(event)\n    ## @end\n]\n";
        let out = splice(text, &[svc("Notes", Some("NotesEvent"), None), svc("Audio", None, Some("AudioEnv"))]).unwrap();
        assert!(out.contains("    Notes(NotesEvent),\n    ## @end"));
        assert!(!out.contains("Audio("));
        assert!(out.starts_with("import NotesEvent\nimport Id\n"));
        let env = "Env := {\n\twidth : U64,\n\t## @trantor(env)\n\t## @end\n}\n";
        let out = splice(env, &[svc("Audio", None, Some("AudioEnv"))]).unwrap();
        assert!(out.contains("\taudio : AudioEnv,\n\t## @end"));
        assert!(out.starts_with("import AudioEnv\nEnv := {"));
    }

    #[test]
    fn unterminated_marker_is_an_error() {
        assert!(splice("Cmd := [\n\t## @trantor(cmd)\n]\n", &[]).is_err());
    }

    #[test]
    fn counts_union_variants_and_fields() {
        let text = "## doc\nNotes := [\n\t## `List(request, key)`\n\tList(U64, Str),\n\tRead(U64, Str, Str),\n\tStop,\n]\n";
        assert_eq!(union_variants(text, "Notes"), Some(vec![2, 3, 0]));
        assert_eq!(union_variants("Http := [Http({ a : U64, b : Str })]", "Http"), Some(vec![1]));
        assert_eq!(union_variants("Env := { ticks : U64 }", "Env"), None);
    }

    #[test]
    fn a_union_needs_a_variant_and_one_is_enough() {
        assert!(check_spliceable("Net := [Http(U64, Str, Str)]", "Net").is_ok());
        assert!(check_spliceable("Net := [Http(U64, Str), Stop]", "Net").is_ok());
        assert!(check_spliceable("Net := []", "Net").is_err());
        assert!(check_spliceable("Env := { ticks : U64 }", "Env").is_err());
    }

    #[test]
    fn snake_cases_module_names() {
        assert_eq!(snake_case("Notes"), "notes");
        assert_eq!(snake_case("NotesEvent"), "notes_event");
        assert_eq!(snake_case("ImgRef"), "img_ref");
    }
}
