//! README.md's Roc examples, proven (D-T3-2, strict per T3b). Every ```roc
//! block builds; what a comment states about a value is compared; and a comment
//! that LOOKS like a stated value but is not one trantor can check fails the
//! run, because a claim that is silently skipped reads the same as one checked.
//!
//! - A whole app (`app [` first) is built and run as written; it must exit with
//!   the fence's `exit=N` (default 0), and a ```text block after it is its
//!   stated stdout.
//! - A module-level block (it defines a function) is compiled; it is not run,
//!   and the report says so.
//! - Everything else is a fragment, stitched into one generated app.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::package_test::{platform_dir, s, Steps, APP_WORLD};
use crate::readme_lex::{blocks, claim, statements, unquote, uses, Block, Claim, Stmt};

const MARK: &str = "@@README@@";
const MARK_ERR: &str = "@@README-ERR@@";

pub fn check(steps: &Steps, with: &Path) -> Result<(), String> {
    let Ok(readme) = std::fs::read_to_string(steps.root.join("README.md")) else { return Ok(()) };
    let all = blocks(&readme)?;
    if all.is_empty() {
        return Ok(());
    }
    let (mut apps, mut modules, mut fragments) = (vec![], vec![], vec![]);
    for b in &all {
        match kind(b) {
            Kind::App => apps.push(b),
            Kind::Module => modules.push(b),
            Kind::Fragment => fragments.push(b),
        }
    }
    let mut with_output = 0;
    for app in &apps {
        with_output += whole_app(steps, with, app)? as usize;
    }
    let module_names = module_level_names(&modules)?;
    let mut compared = 0;
    if !fragments.is_empty() || !modules.is_empty() {
        let prelude_text = std::fs::read_to_string(steps.root.join("tests/readme-prelude.roc")).unwrap_or_default();
        let prelude_lines: Vec<(usize, String)> = prelude_text.lines().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect();
        let prelude = statements(&prelude_lines).0;
        let platform = std::fs::read_to_string(platform_dir(with, APP_WORLD).join("main.roc"))
            .map_err(|e| format!("read the composed platform: {e}"))?;
        let g = generate(&fragments, &modules, &module_names, &prelude, &platform)?;
        let got = run_source(steps, with, &g.app, "the README.md examples", 0)?;
        compared = compare(&g.expected, &got)?;
    }
    println!(
        "ok: README.md — {} whole apps run ({with_output} with stated output), {} module-level blocks compile (not run), {} fragment blocks run; {compared} stated values compared",
        apps.len(), modules.len(), fragments.len()
    );
    Ok(())
}

enum Kind { App, Module, Fragment }

fn kind(b: &Block) -> Kind {
    let meaningful = b.lines.iter().map(|(_, l)| l.trim()).find(|l| !l.is_empty() && !l.starts_with('#'));
    if meaningful.is_some_and(|l| l.starts_with("app [")) {
        return Kind::App;
    }
    // A function defined at column 0: `name = |...|` or `name! = |...|`.
    let defines_function = b.lines.iter().any(|(_, l)| {
        l.split_once(" = ").is_some_and(|(lhs, rhs)| is_ident(lhs.trim_end_matches('!')) && !l.starts_with(char::is_whitespace) && rhs.trim_start().starts_with('|'))
    });
    if defines_function { Kind::Module } else { Kind::Fragment }
}

/// Build and run a whole app; true when it stated its output.
fn whole_app(steps: &Steps, with: &Path, app: &Block) -> Result<bool, String> {
    for (n, raw) in &app.lines {
        if let Some(c) = crate::readme_lex::scan(raw).comment_at.map(|at| raw[at..].trim_start_matches('#').trim().to_string()) {
            if claim(&c) != Claim::Prose {
                return Err(format!("README.md line {n}: `# {c}` states a value inside a whole app; state its output in a ```text block after the app instead"));
            }
        }
    }
    let source = retarget(&app.lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n"));
    let what = format!("the README.md app at line {}", app.first_line);
    let got = run_source(steps, with, &source, &what, app.exit)?;
    if let Some(stated) = &app.output {
        if &got != stated {
            return Err(format!("{what} printed something other than the output stated after it\n  stated:\n{}\n  actual:\n{}", indent(stated), indent(&got)));
        }
    }
    Ok(app.output.is_some())
}

/// Build and run `source` as the scratch world's app, requiring `exit`.
fn run_source(steps: &Steps, with: &Path, source: &str, what: &str, exit: i32) -> Result<String, String> {
    std::fs::write(with.join("app/main.roc"), source).map_err(|e| format!("write {what}: {e}"))?;
    let build = steps.trantor(&["build", s(with), "--app", "app", "--out", "readme"], &format!("{what} does not build"))?;
    drop(build);
    let bin = with.join("target/trantor").join(APP_WORLD).join("bin/readme");
    let ran = crate::bounded::run(std::process::Command::new(&bin).current_dir(with), what, &steps.scratch.join("runs"))?;
    if let Some(secs) = ran.timed_out_after {
        return Err(format!("{what} was killed after {secs} s"));
    }
    match ran.status.and_then(|s| s.code()) {
        Some(code) if code == exit => Ok(ran.stdout),
        other => Err(format!("{what} exited {other:?}, expected {exit}\n{}{}", ran.stdout, ran.stderr)),
    }
}

/// A README app names its reader's world; point it at the test world. The
/// header may span lines, so the first `platform "..."` is the one replaced.
pub fn retarget(source: &str) -> String {
    let Some(at) = source.find("platform \"") else { return format!("{source}\n") };
    let rest = &source[at + "platform \"".len()..];
    let Some(end) = rest.find('"') else { return format!("{source}\n") };
    format!("{}platform \"../target/trantor/{APP_WORLD}/platform/main.roc\"{}\n", &source[..at], &rest[end + 1..])
}

/// Names module-level blocks define, refusing one defined twice: the blocks
/// share one app, and two definitions of a name do not compile.
fn module_level_names(modules: &[&Block]) -> Result<BTreeMap<String, usize>, String> {
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for b in modules {
        for (n, l) in &b.lines {
            if l.starts_with(char::is_whitespace) { continue }
            let Some((lhs, _)) = l.split_once(" = ") else { continue };
            if !is_ident(lhs.trim_end_matches('!')) { continue }
            if let Some(first) = names.insert(lhs.to_string(), *n) {
                if first != *n {
                    return Err(format!("README.md defines `{lhs}` twice, at lines {first} and {n}; the module-level examples share one app"));
                }
            }
        }
    }
    Ok(names)
}

pub struct Generated {
    pub app: String,
    /// README line -> what it claims.
    pub expected: BTreeMap<usize, Claim>,
}

pub fn generate(fragments: &[&Block], modules: &[&Block], module_names: &BTreeMap<String, usize>, prelude: &[Stmt], platform: &str) -> Result<Generated, String> {
    let contract = crate::scaffold::main_contract(platform)?;
    let mut imports: BTreeSet<String> = contract.imports.iter().cloned().collect();
    imports.insert("pf.Stdout".into());
    let (mut top, mut fns, mut calls) = (vec![], vec![], vec![]);
    let mut expected = BTreeMap::new();
    for b in modules {
        for (n, l) in &b.lines {
            if let Some(m) = l.strip_prefix("import ") { imports.insert(m.trim().to_string()); continue }
            if let Some(at) = crate::readme_lex::scan(l).comment_at {
                let c = l[at..].trim_start_matches('#').trim();
                if claim(c) != Claim::Prose {
                    return Err(format!("README.md line {n}: `# {c}` states a value in a module-level block, which is compiled but not run"));
                }
            }
        }
        top.push(format!("# README.md line {}", b.first_line));
        top.extend(b.lines.iter().filter(|(_, l)| !l.starts_with("import ")).map(|(_, l)| l.clone()));
    }
    for (index, b) in fragments.iter().enumerate() {
        imports.extend(b.lines.iter().filter_map(|(_, l)| l.strip_prefix("import ").map(|m| m.trim().to_string())));
        let body: Vec<(usize, String)> = b.lines.iter().filter(|(_, l)| !l.starts_with("import ")).cloned().collect();
        let (stmts, loose) = statements(&body);
        for (n, c) in &loose {
            if claim(c) != Claim::Prose {
                return Err(format!("README.md line {n}: `# {c}` states a value but follows no expression"));
            }
        }
        let bound: Vec<Vec<String>> = stmts.iter().map(|st| binding(st).map(|(names, _)| names).unwrap_or_default()).collect();
        let defined: BTreeSet<String> = bound.iter().flatten().cloned().chain(module_names.keys().cloned()).collect();
        let clock = clock_dependent(&stmts, &bound);
        // A block that binds nothing is a list of independent calls, which may
        // mix error types no single `?` can carry, so each stands alone.
        let units: Vec<Vec<usize>> = if bound.iter().all(|b| b.is_empty()) { (0..stmts.len()).map(|i| vec![i]).collect() } else { vec![(0..stmts.len()).collect()] };
        for (part, unit) in units.iter().enumerate() {
            let mut body_lines = injected_prelude(prelude, unit.iter().map(|&i| stmts[i].blanked.as_str()), &defined);
            for &i in unit {
                body_lines.extend(statement(&stmts[i], &bound[i], clock[i], &mut expected)?);
            }
            let first = stmts[unit[0]].line;
            let name = format!("block_{index}_{part}!");
            fns.push(format!("# README.md line {first}\n{name} : {{}} => Try({{}}, _)\n{name} = |{{}}| {{\n{}\tOk({{}})\n}}\n",
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
fn statement(st: &Stmt, bound: &[String], clock: bool, expected: &mut BTreeMap<usize, Claim>) -> Result<Vec<String>, String> {
    let claims: Vec<(usize, String, Claim)> = st.comments.iter().map(|(n, c)| (*n, c.clone(), claim(c))).filter(|(_, _, c)| *c != Claim::Prose).collect();
    for (n, c, cl) in &claims {
        if let Claim::Unrecognised(head) = cl {
            return Err(format!("README.md line {n}: `# {c}` looks like a stated value, but `{head}` is not one trantor can check. State it as \"text\", a single token (3, 2024-02-29, P359D, true), Ok(...) or Err(...), with any explanation after ` — `"));
        }
        if clock {
            return Err(format!("README.md line {n}: `# {c}` states a value for a line that reads the clock, which changes between runs; say it in prose"));
        }
    }
    if claims.len() > 1 {
        return Err(format!("README.md line {}: more than one stated value for one expression", st.line));
    }
    let is_annotation = binding(st).is_none() && top_level(&st.blanked, ':').is_some() && !st.code.contains("::");
    let (lines, var, rendered_code) = match binding(st) {
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
    let mut lines = lines;
    if let (Some((n, _, cl)), Some(var)) = (claims.into_iter().next(), var) {
        let shown = match &cl {
            Claim::Tag(_) => format!("Str.inspect({var})"),
            _ if renders_str(&rendered_code) => format!("Str.inspect({var})"),
            _ => format!("Str.inspect({var}.to_str())"),
        };
        lines.push(format!("Stdout.line!(\"{MARK}:{n}:${{{shown}}}\") ?? {{}}"));
        expected.insert(n, cl);
    }
    Ok(lines)
}

/// `(names bound, right-hand side)` for `pattern = expr`.
fn binding(st: &Stmt) -> Option<(Vec<String>, String)> {
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
fn top_level(blanked: &str, target: char) -> Option<usize> {
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

/// Statements that read the clock, directly or through a binding that did.
fn clock_dependent(stmts: &[Stmt], bound: &[Vec<String>]) -> Vec<bool> {
    let mut tainted: BTreeSet<String> = BTreeSet::new();
    stmts.iter().zip(bound).map(|(st, names)| {
        let reads = st.blanked.contains("Now.") || tainted.iter().any(|t| uses(&st.blanked, t));
        if reads { tainted.extend(names.iter().cloned()); }
        reads
    }).collect()
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

/// Compare the generated app's marked output against the claims; the count.
fn compare(expected: &BTreeMap<usize, Claim>, stdout: &str) -> Result<usize, String> {
    let mut got: BTreeMap<usize, String> = BTreeMap::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{MARK_ERR}:")) {
            let (n, e) = rest.split_once(':').unwrap_or((rest, ""));
            return Err(format!("README.md line {n}: the example returned an error: {e}"));
        }
        if let Some((n, v)) = line.strip_prefix(&format!("{MARK}:")).and_then(|r| r.split_once(':')) {
            if let Ok(n) = n.parse() { got.insert(n, v.to_string()); }
        }
    }
    let mut wrong = vec![];
    for (n, cl) in expected {
        let actual = got.get(n).ok_or_else(|| format!("README.md line {n}: the example printed no value for its stated one"))?;
        let (stated, shown) = match cl {
            Claim::Tag(t) => (t.clone(), actual.clone()),
            Claim::Quoted(t) | Claim::Token(t) => (t.clone(), rendered(actual)),
            _ => continue,
        };
        if stated != shown {
            wrong.push(format!("  README.md line {n}: stated {stated:?}, actual {shown:?}"));
        }
    }
    if !wrong.is_empty() {
        return Err(format!("README.md says one thing and the package does another\n{}", wrong.join("\n")));
    }
    Ok(expected.len())
}

/// A `Str.inspect` of a Str, or of an `Ok` holding one, as the plain text.
fn rendered(inspected: &str) -> String {
    let inner = inspected.strip_prefix("Ok(").and_then(|r| r.strip_suffix(')')).unwrap_or(inspected);
    inner.strip_prefix('"').and_then(unquote).filter(|(_, tail)| tail.is_empty()).map(|(t, _)| t).unwrap_or_else(|| inspected.to_string())
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}

/// Whether an expression's last call already yields a `Str`: `to_str` or
/// `format`, effectful or not.
pub fn renders_str(code: &str) -> bool {
    let code = code.trim();
    let code = code.strip_suffix('?').unwrap_or(code);
    let Some(inner) = code.strip_suffix(')') else { return false };
    let mut depth = 1;
    let open = inner.char_indices().rev().find_map(|(i, c)| {
        match c { ')' => depth += 1, '(' => depth -= 1, _ => {} }
        (depth == 0).then_some(i)
    });
    open.is_some_and(|i| [".to_str", ".format", ".format!"].iter().any(|m| inner[..i].ends_with(m)) && !inner[..i].ends_with(".to_str!"))
}

fn is_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_str_or_ok_str_inspection_renders_as_its_text() {
        assert_eq!(rendered(r#""2024-02-29""#), "2024-02-29");
        assert_eq!(rendered(r#"Ok("P359D")"#), "P359D");
        assert_eq!(rendered(r#""say \"hi\"""#), "say \"hi\"");
        assert_eq!(rendered("Err(TooLarge)"), "Err(TooLarge)");
    }

    #[test]
    fn only_a_final_to_str_or_format_call_already_renders() {
        assert!(renders_str("jan31.to_str()") && renders_str(r#"march8.format("%A %e")"#) && renders_str(r#"z.format!("%H")?"#));
        assert!(!renders_str("jan31.add!({ months: 1 })?") && !renders_str("f(x.to_str())") && !renders_str("jan31.year"));
        assert!(!renders_str("after.to_str!()"), "to_str! without ? is a Try, not a Str");
    }

    #[test]
    fn top_level_assignment_ignores_comparisons_and_nesting() {
        assert_eq!(top_level("x = a == b", '='), Some(2));
        assert_eq!(top_level("f({ a: 1 }) == g", '='), None);
        assert_eq!(top_level("{ year, month } = jan31", '='), Some(16));
        assert_eq!(top_level("message : Str", ':'), Some(8));
    }

    #[test]
    fn a_whole_app_header_is_retargeted_even_across_lines() {
        let src = "app [main!] {\n\tpf: platform \"../target/trantor/myapp/platform/main.roc\",\n}\nmain! = |_| Ok({})";
        assert!(retarget(src).contains("platform \"../target/trantor/app/platform/main.roc\","));
    }
}
