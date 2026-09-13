//! Service unions with one variant, made to work in the glue output (D-H7-44,
//! superseding D-H7-21). `roc glue` unwraps a single-variant union to its
//! payload, so the named type a service and the shim use does not exist, and
//! a multi-field payload is mis-typed as `u64` — its size asserts then fail
//! and its refcounts are skipped. Measured on release-fast-10e922df:
//!
//! - one field (`[Shout(Str)]`): the wrapper's payload is typed correctly; only
//!   the name is missing, so `pub type Echo = RocStr;` is all it needs.
//! - no field (`[Stop]`): the payload is zero-sized and glue writes no
//!   accessor; trantor adds a unit struct and the accessors the shim calls.
//! - several fields (`[Ping(U64, Str, Str)]`): trantor runs glue a second time
//!   on a copy of the platform where that union has a placeholder second
//!   variant, takes the payload struct glue writes there, and puts it in place
//!   of the `u64` — field, accessors and refcount arms. The placeholder never
//!   reaches the app or the service: only this second glue run sees it.
//!
//! The service keeps the API it would have had; composition absorbs glue's
//! behaviour, instead of every one-command service growing a second command.
use std::path::Path;

use crate::resolve::Service;
use crate::splice::snake_case;

/// The placeholder variant the second glue run sees.
const PLACEHOLDER: &str = "TrantorGlueOnly";

pub struct Single {
    /// The union's module name, which glue would have named the type.
    pub module: String,
    pub variant: String,
    pub fields: usize,
    /// `Cmd` or `Event`: the driver union the service wraps it in.
    pub block: &'static str,
    /// The wrapper variant's field name in that union (`echo`).
    pub wrapper: String,
}

/// The single-variant unions of a world's services, read from the composed
/// platform's module copies.
pub fn singles(platform: &Path, services: &[Service]) -> Result<Vec<Single>, String> {
    let mut out = vec![];
    for svc in services {
        let blocks = [("Cmd", Some(&svc.module)), ("Event", svc.event_module.as_ref())];
        for (block, module) in blocks {
            let Some(module) = module else { continue };
            let p = platform.join(format!("{module}.roc"));
            let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
            let Some((variant, fields)) = only_variant(&text, module) else { continue };
            out.push(Single { module: module.clone(), variant, fields, block, wrapper: snake_case(&svc.module) });
        }
    }
    Ok(out)
}

/// `(name, field count)` when `name := [...]` has exactly one variant.
fn only_variant(text: &str, name: &str) -> Option<(String, usize)> {
    let counts = crate::splice::union_variants(text, name)?;
    let [fields] = counts.as_slice() else { return None };
    let start = text.find(&format!("{name} :="))?;
    let body = &text[text[start..].find('[')? + start + 1..];
    let variant: String = body
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .find(|l| !l.is_empty())?
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!variant.is_empty()).then_some((variant, *fields))
}

/// The module text with a placeholder second variant, for the second glue run.
pub fn with_placeholder(text: &str, name: &str) -> Option<String> {
    let start = text.find(&format!("{name} :="))?;
    let open = text[start..].find('[')? + start;
    let mut depth = 0i32;
    let mut in_comment = false;
    for (i, c) in text[open..].char_indices() {
        match c {
            _ if in_comment => in_comment = c != '\n',
            '#' => in_comment = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    let at = open + i;
                    let before = text[..at].trim_end();
                    let sep = if before.ends_with(',') || before.ends_with('[') { "" } else { "," };
                    return Some(format!("{before}{sep} {PLACEHOLDER}{}", &text[at..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Glue's output with every single-variant union made usable. `placeholder`
/// is glue's output for the placeholder copy, needed only for multi-field
/// unions.
pub fn repair(glue: &str, singles: &[Single], placeholder: Option<&str>) -> Result<String, String> {
    let mut out = glue.to_string();
    let mut appendix = String::from("\n// ---- trantor: single-variant service unions (see trantor's src/glue_unions.rs) ----\n");
    for s in singles {
        match s.fields {
            0 => {
                appendix.push_str(&format!(
                    "\n/// `{m}` has one variant, `{v}`, with no payload.\n#[repr(C)]\n#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]\npub struct {m};\n\
                     impl {m} {{\n    pub unsafe fn decref(self, _roc_host: &RocHost) {{}}\n    pub unsafe fn incref(self, _amount: isize) {{}}\n}}\n\
                     impl {b} {{\n    pub unsafe fn take_payload_{w}_unchecked(&mut self) -> {m} {{ {m} }}\n    pub unsafe fn borrow_payload_{w}_unchecked(&self) -> &{m} {{ &{m} }}\n}}\n",
                    m = s.module, v = s.variant, b = s.block, w = s.wrapper
                ));
            }
            1 => {
                let ty = accessor_type(&out, s).ok_or_else(|| format!("glue output has no accessor for `{}`'s payload in {}", s.module, s.block))?;
                appendix.push_str(&format!("\n/// `{m}` has one variant, `{v}`; glue passes its payload as is.\npub type {m} = {ty};\n", m = s.module, v = s.variant));
            }
            _ => {
                let placeholder = placeholder.ok_or("a multi-field single-variant union needs the placeholder glue run")?;
                let payload = format!("{}{}Payload", s.module, s.variant);
                appendix.push_str(&payload_items(placeholder, s, &payload)?);
                appendix.push_str(&format!("\n/// `{m}` has one variant, `{v}`, with several fields.\npub type {m} = {payload};\n", m = s.module, v = s.variant));
                out = retype_wrapper(&out, s, &payload)?;
            }
        }
    }
    if !singles.is_empty() {
        out.push_str(&appendix);
    }
    Ok(out)
}

/// The type `impl {block}`'s `take_payload_{wrapper}_unchecked` returns.
fn accessor_type(glue: &str, s: &Single) -> Option<String> {
    let needle = format!("fn take_payload_{}_unchecked(&mut self) -> ", s.wrapper);
    in_impl(glue, s.block).find_map(|l| l.split_once(&needle).map(|(_, t)| t.trim_end_matches('{').trim().to_string()))
}

/// Lines inside `impl {block} {` blocks.
fn in_impl<'a>(glue: &'a str, block: &str) -> impl Iterator<Item = &'a str> {
    let header = format!("impl {block} {{");
    let mut inside = false;
    glue.lines().filter(move |l| {
        if l.starts_with("impl ") {
            inside = *l == header;
        } else if *l == "}" && inside {
            inside = false;
            return false;
        }
        inside
    })
}

/// From the placeholder run: the payload struct (both pointer widths, with its
/// asserts), and its refcount methods built from glue's own arms for it.
fn payload_items(placeholder: &str, s: &Single, payload: &str) -> Result<String, String> {
    let lines: Vec<&str> = placeholder.lines().collect();
    let mut items = String::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let starts_struct = l == format!("pub struct {payload} {{");
        let asserts = l.starts_with("const _: () = assert!") && l.contains(&format!("<{payload}>"));
        if starts_struct {
            // Its doc, cfg and derive lines sit just above.
            let mut head = i;
            while head > 0 && (lines[head - 1].starts_with("#[") || lines[head - 1].starts_with("///")) {
                head -= 1;
            }
            let mut end = i;
            while end < lines.len() && lines[end] != "}" {
                end += 1;
            }
            items.push('\n');
            items.push_str(&lines[head..=end.min(lines.len() - 1)].join("\n"));
            items.push('\n');
            i = end + 1;
            continue;
        }
        if asserts {
            if let Some(cfg) = i.checked_sub(1).map(|j| lines[j]).filter(|c| c.starts_with("#[cfg")) {
                items.push_str(cfg);
                items.push('\n');
            }
            items.push_str(l);
            items.push('\n');
        }
        i += 1;
    }
    if !items.contains(&format!("pub struct {payload}")) {
        return Err(format!("the placeholder glue run wrote no `{payload}`"));
    }
    let arm = |f: &str| refcount_arm(placeholder, &s.module, &s.variant, f);
    let (dec, inc) = (arm("decref").unwrap_or_default(), arm("incref").unwrap_or_default());
    items.push_str(&format!(
        "\nimpl {payload} {{\n    pub unsafe fn decref(self, roc_host: &RocHost) {{\n        let _ = roc_host;\n        let payload = self;\n{dec}    }}\n\
         \x20   pub unsafe fn incref(self, amount: isize) {{\n        let _ = amount;\n        let payload = self;\n{inc}    }}\n}}\n"
    ));
    Ok(items)
}

/// The statements glue's `fn {f}` in `impl {module}` runs for `{module}Tag::{variant}`,
/// after it binds `payload`.
fn refcount_arm(glue: &str, module: &str, variant: &str, f: &str) -> Option<String> {
    let mut lines = in_impl(glue, module).skip_while(|l| !l.contains(&format!("pub unsafe fn {f}(")));
    let arm = format!("{module}Tag::{variant} => {{");
    lines.find(|l| l.trim() == arm)?;
    let mut body = String::new();
    for l in lines {
        let t = l.trim();
        if t == "}," || t == "}" {
            break;
        }
        if !t.starts_with("let payload") {
            body.push_str(&format!("        {t}\n"));
        }
    }
    Some(body)
}

/// Put `payload` in place of the `u64` glue gave the wrapper, in the payload
/// union, the accessors, and the refcount arms glue left empty.
fn retype_wrapper(glue: &str, s: &Single, payload: &str) -> Result<String, String> {
    let (w, b) = (&s.wrapper, s.block);
    let field = format!("pub {w}: core::mem::ManuallyDrop<u64>,");
    if !glue.contains(&field) {
        return Err(format!("glue output does not type `{b}`'s `{w}` payload as u64 — its handling of single-variant unions changed; re-measure (D-H7-44)"));
    }
    let tag_arm = format!("{b}Tag::{} => {{}},", s.module);
    let mut out = Vec::new();
    let mut in_block_impl = false;
    let mut current_fn = "";
    for l in glue.lines() {
        if l.starts_with("impl ") {
            in_block_impl = l == format!("impl {b} {{");
        }
        if let Some(f) = ["decref", "incref"].into_iter().find(|f| l.contains(&format!("pub unsafe fn {f}("))) {
            current_fn = f;
        }
        let mut line = l.replace(&field, &format!("pub {w}: core::mem::ManuallyDrop<{payload}>,"));
        if in_block_impl && line.contains(&format!("_{w}_unchecked")) {
            line = line.replace("-> &u64", &format!("-> &{payload}")).replace("-> u64", &format!("-> {payload}"));
        }
        if in_block_impl && line.contains(&format!("ManuallyDrop<u64> as *const u64")) && out.last().is_some_and(|p: &String| p.contains(&format!("_{w}_unchecked"))) {
            line = line.replace("ManuallyDrop<u64> as *const u64", &format!("ManuallyDrop<{payload}> as *const {payload}"));
        }
        if in_block_impl && line.trim() == tag_arm {
            let indent = &line[..line.len() - line.trim_start().len()];
            line = match current_fn {
                "decref" => format!("{indent}{b}Tag::{m} => {{\n{indent}    let payload = unsafe {{ value.take_payload_{w}_unchecked() }};\n{indent}    unsafe {{ payload.decref(roc_host); }}\n{indent}}},", m = s.module),
                _ => format!("{indent}{b}Tag::{m} => {{\n{indent}    let payload = unsafe {{ core::ptr::read(value.borrow_payload_{w}_unchecked()) }};\n{indent}    unsafe {{ payload.incref(amount); }}\n{indent}}},", m = s.module),
            };
        }
        out.push(line);
    }
    Ok(out.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_variant_is_found_with_its_name_and_field_count() {
        assert_eq!(only_variant("Echo := [\n\t## doc\n\tPing(U64, Str, Str),\n]", "Echo"), Some(("Ping".into(), 3)));
        assert_eq!(only_variant("Tick := [Stop]", "Tick"), Some(("Stop".into(), 0)));
        assert_eq!(only_variant("Echo := [Ping(U64), Shout(Str)]", "Echo"), None);
    }

    #[test]
    fn the_placeholder_is_a_second_variant() {
        assert_eq!(with_placeholder("Echo := [\n\tPing(U64, Str, Str),\n]\n", "Echo").unwrap(), format!("Echo := [\n\tPing(U64, Str, Str), {PLACEHOLDER}]\n"));
        assert_eq!(with_placeholder("Tick := [Stop]", "Tick").unwrap(), format!("Tick := [Stop, {PLACEHOLDER}]"));
    }
}
