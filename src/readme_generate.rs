//! README fragments and module-level blocks stitched into one app whose
//! output states each claimed value (D-T3-2, T3b).
use std::collections::{BTreeMap, BTreeSet};

use crate::package_test::APP_WORLD;
use crate::readme_claims::{claim, shown, via_to_str, Claim};
use crate::readme_lex::{scan, statements, Block, Stmt};
use crate::readme_taint::{uses, Taint};

pub const MARK: &str = "@@README@@";
pub const MARK_END: &str = "@@README-END@@";
pub const MARK_ERR: &str = "@@README-ERR@@";
/// A generated-app comment naming the README line the code below came from.
pub const LINE_TAG: &str = "# README.md line ";

pub struct Generated {
    pub app: String,
    /// README line -> what it claims, and whether it is shown via `to_str`.
    pub expected: BTreeMap<usize, (Claim, bool)>,
}

/// `import X [as Y] [exposing [...]]` without its comment.
pub fn import_of(line: &str) -> Option<String> {
    let code = &line[..scan(line).comment_at.unwrap_or(line.len())];
    code.strip_prefix("import ").map(|m| m.trim().to_string())
}

pub fn generate(fragments: &[&Block], modules: &[&Block], module_names: &BTreeMap<String, usize>, prelude: &[Stmt], platform: &str) -> Result<Generated, String> {
    let contract = crate::main_contract::main_contract(platform)?;
    if contract.body != "Ok({})" {
        return Err(format!("README.md examples run inside a generated main!, which must be able to finish; this baseline's is `{}`", contract.signature));
    }
    let mut imports: BTreeSet<String> = contract.imports.iter().cloned().collect();
    imports.insert("pf.Stdout".into());
    imports.extend(modules.iter().chain(fragments).flat_map(|b| b.lines.iter().filter_map(|(_, l)| import_of(l))));
    let taint = Taint::of(&imports, modules, prelude);
    let mut top = vec![];
    for b in modules {
        for (n, l) in &b.lines {
            if let Some(at) = scan(l).comment_at {
                let c = l[at..].trim_start_matches('#').trim();
                if claim(c) != Claim::Prose {
                    return Err(format!("README.md line {n}: `# {c}` states a value in a module-level block, which is compiled but not run"));
                }
            }
        }
        top.push(format!("{LINE_TAG}{}", b.first_line));
        // An import line stays as a blank one, so line offsets still map back.
        top.extend(b.lines.iter().map(|(_, l)| if import_of(l).is_some() { String::new() } else { l.clone() }));
    }
    let (mut fns, mut calls, mut expected) = (vec![], vec![], BTreeMap::new());
    for (index, b) in fragments.iter().enumerate() {
        let body: Vec<(usize, String)> = b.lines.iter().filter(|(_, l)| import_of(l).is_none()).cloned().collect();
        let (stmts, loose) = statements(&body);
        for (n, c) in &loose {
            if claim(c) != Claim::Prose {
                return Err(format!("README.md line {n}: `# {c}` states a value but follows no expression"));
            }
        }
        let bound: Vec<Vec<String>> = stmts.iter().map(|st| binding(st).map(|(names, _)| names).unwrap_or_default()).collect();
        let defined: BTreeSet<String> = bound.iter().flatten().cloned().chain(module_names.keys().cloned()).collect();
        let clock = taint.dependent(&stmts, &bound);
        // A block that binds nothing is a list of independent calls, which may
        // mix error types no single `?` can carry, so each stands alone.
        let units: Vec<Vec<usize>> = if bound.iter().all(|b| b.is_empty()) { (0..stmts.len()).map(|i| vec![i]).collect() } else { vec![(0..stmts.len()).collect()] };
        for (part, unit) in units.iter().enumerate() {
            let mut body_lines = injected_prelude(prelude, unit.iter().map(|&i| stmts[i].blanked.as_str()), &defined);
            for (k, &i) in unit.iter().enumerate() {
                body_lines.push(format!("{LINE_TAG}{}", stmts[i].line));
                body_lines.extend(statement(&stmts[i], &bound[i], clock[i], &mut expected)?);
                // Roc warns on a binding nothing uses, and a warning fails the
                // build; an example may well show a binding on its own.
                let later: Vec<&str> = unit[k + 1..].iter().map(|&j| stmts[j].blanked.as_str()).collect();
                let claimed = stmts[i].comments.iter().any(|(_, c)| claim(c) != Claim::Prose);
                for name in &bound[i] {
                    if !name.starts_with('_') && !claimed && !later.iter().any(|l| uses(l, name)) {
                        body_lines.push(format!("_ = {name}"));
                    }
                }
            }
            let first = stmts[unit[0]].line;
            let name = format!("block_{index}_{part}!");
            fns.push(format!("{LINE_TAG}{first}\n{name} : {{}} => Try({{}}, _)\n{name} = |{{}}| {{\n{}\tOk({{}})\n}}\n",
                body_lines.iter().map(|l| format!("\t{}\n", l.replace('\n', "\n\t"))).collect::<String>()));
            calls.push(format!("\tmatch {name}({{}}) {{ Ok(_) => {{}}, Err(e) => Stdout.line!(\"{MARK_ERR}:{first}:${{Str.inspect(e)}}\") ?? {{}} }}"));
        }
    }
    let app = format!(
        "app [main!] {{ pf: platform \"../target/trantor/{APP_WORLD}/platform/main.roc\" }}\n\n{}\n\n{}\n\n{}\n{}\nmain! = |{}| {{\n{}\n\t{}\n}}\n",
        imports.iter().map(|i| format!("import {i}")).collect::<Vec<_>>().join("\n"),
        top.join("\n"), fns.join("\n"), contract.signature, contract.param, calls.join("\n"), contract.body,
    );
    Ok(Generated { app, expected })
}

/// A statement's generated line(s), recording what its comments claim.
fn statement(st: &Stmt, bound: &[String], clock: bool, expected: &mut BTreeMap<usize, (Claim, bool)>) -> Result<Vec<String>, String> {
    for (n, c) in &st.inner {
        if claim(c) != Claim::Prose {
            return Err(format!("README.md line {n}: `# {c}` is inside a multi-line expression, so it cannot state that expression's value; state it after the closing line"));
        }
    }
    let claims: Vec<(usize, String, Claim)> = st.comments.iter().map(|(n, c)| (*n, c.clone(), claim(c))).filter(|(_, _, c)| *c != Claim::Prose).collect();
    for (n, c, cl) in &claims {
        if let Claim::Unrecognised(head) = cl {
            return Err(format!("README.md line {n}: `# {c}` looks like a stated value, but `{head}` is not one trantor can check. State it as \"text\", a single token (3, True, LT, 2024-02-29, P359D), or Name(...) such as Ok(...) or Err(...), with any explanation after ` — `"));
        }
        if clock {
            return Err(format!("README.md line {n}: `# {c}` states a value for a line that reads the clock or the machine, which changes between runs; say it in prose"));
        }
    }
    if claims.len() > 1 {
        return Err(format!("README.md line {}: more than one stated value for one expression", st.line));
    }
    if st.code.trim_start().starts_with("expect") {
        if !claims.is_empty() { return Err(format!("README.md line {}: an expect cannot state a value", st.line)) }
        return Ok(vec![st.code.clone()]);
    }
    let is_annotation = binding(st).is_none() && top_level(&st.blanked, ':').is_some() && !st.code.contains("::");
    let (mut lines, var, rendered_code) = match binding(st) {
        Some(_) if is_annotation => (vec![st.code.clone()], None, String::new()),
        Some((_, rhs)) => {
            let var = (bound.len() == 1).then(|| bound[0].clone());
            if !claims.is_empty() && var.is_none() {
                return Err(format!("README.md line {}: a destructuring binding cannot state a value; bind a name and state it there", st.line));
            }
            (vec![st.code.clone()], var, rhs)
        }
        None if is_annotation => {
            if !claims.is_empty() { return Err(format!("README.md line {}: a type annotation cannot state a value", st.line)) }
            (vec![st.code.clone()], None, String::new())
        }
        None => {
            let var = if claims.is_empty() { format!("_r{}", st.line) } else { format!("r{}", st.line) };
            (vec![format!("{var} = {}", st.code)], Some(var), st.code.clone())
        }
    };
    if let (Some((n, _, cl)), Some(var)) = (claims.into_iter().next(), var) {
        let via = via_to_str(&cl, &rendered_code);
        lines.push(format!("Stdout.line!(\"{MARK}:{n}:${{{}}}{MARK_END}\") ?? {{}}", shown(&var, via)));
        expected.insert(n, (cl, via));
    }
    Ok(lines)
}

/// `(names bound, right-hand side)` for `pattern = expr`.
pub fn binding(st: &Stmt) -> Option<(Vec<String>, String)> {
    let at = top_level(&st.blanked.replace('\n', " "), '=')?;
    let flat = st.code.replace('\n', " ");
    let lhs = flat.get(..at)?.trim();
    let rhs = st.code.split_once('=').map(|(_, r)| r.trim().to_string()).unwrap_or_default();
    let names = lhs.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '!'))
        .filter(|w| is_ident(w.trim_end_matches('!')))
        .map(str::to_string)
        .collect();
    Some((names, rhs))
}

/// A top-level `=` (not `==`, `!=`, `<=`, `>=`, `=>`) or `:` (not `::`), outside brackets.
pub fn top_level(blanked: &str, target: char) -> Option<usize> {
    let b: Vec<char> = blanked.chars().collect();
    let mut depth = 0;
    let mut byte = 0;
    for (i, &c) in b.iter().enumerate() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && c == target {
            let (prev, next) = (i.checked_sub(1).map(|j| b[j]), b.get(i + 1).copied());
            let ok = match target {
                '=' => !matches!(prev, Some('=' | '!' | '<' | '>')) && !matches!(next, Some('=' | '>')),
                _ => prev != Some(':') && next != Some(':'),
            };
            if ok { return Some(byte) }
        }
        byte += c.len_utf8();
    }
    None
}

pub fn is_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Prelude bindings a unit uses and does not define, and the prelude bindings
/// those use in turn, in prelude order.
fn injected_prelude<'a>(prelude: &[Stmt], unit: impl Iterator<Item = &'a str>, defined: &BTreeSet<String>) -> Vec<String> {
    let text: String = unit.collect::<Vec<_>>().join(" ");
    let named: Vec<(String, &Stmt)> = prelude.iter().filter_map(|p| binding(p).and_then(|(n, _)| n.first().cloned()).map(|n| (n, p))).collect();
    let mut wanted: BTreeSet<String> = named.iter().filter(|(n, _)| !defined.contains(n) && uses(&text, n)).map(|(n, _)| n.clone()).collect();
    loop {
        let more: Vec<String> = named.iter()
            .filter(|(n, _)| wanted.contains(n))
            .flat_map(|(_, p)| named.iter().filter(|(m, _)| !defined.contains(m) && uses(&p.blanked, m)).map(|(m, _)| m.clone()))
            .filter(|m| !wanted.contains(m))
            .collect();
        if more.is_empty() { break }
        wanted.extend(more);
    }
    named.iter().filter(|(n, _)| wanted.contains(n)).map(|(_, p)| p.code.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_assignment_ignores_comparisons_and_nesting() {
        assert_eq!(top_level("x = a == b", '='), Some(2));
        assert_eq!(top_level("f({ a: 1 }) == g", '='), None);
        assert_eq!(top_level("{ year, month } = jan31", '='), Some(16));
        assert_eq!(top_level("message : Str", ':'), Some(8));
    }


}
