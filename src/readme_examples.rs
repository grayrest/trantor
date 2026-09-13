//! README.md's Roc examples, proven (D-T3-2, strict per T3b). Every ```roc
//! block builds; what a comment states about a value is compared; and a comment
//! that LOOKS like a stated value but is not one trantor can check fails the
//! run, because a claim that is silently skipped reads the same as one checked.
//!
//! - A whole app (`app [` first) is built and run as written; it must exit with
//!   the fence's `exit=N` (default 0), and a ```text block after it — prose
//!   between is fine — is its stated stdout.
//! - A module-level block (definitions only, a function or a type among them)
//!   is compiled; it is not run, and the report says so.
//! - Everything else is a fragment, stitched into one generated app.
use std::collections::BTreeMap;
use std::path::Path;

use crate::package_test::{platform_dir, s, Steps, APP_WORLD};
use crate::readme_claims::{matches, Claim};
use crate::readme_generate::{binding, claim_at, defines_main, generate, import_of, is_ident, top_level, GEN_TAG, LINE_TAG, MARK, MARK_END, MARK_ERR};
use crate::readme_lex::{blocks, statements, Block};

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
            _ if defines_main(b).is_some() => {
                return Err(format!("README.md line {}: this block defines `main!` without an `app [main!] {{ ... }}` header; make it a whole app so it runs as written", defines_main(b).unwrap_or(b.first_line)));
            }
            kind => {
                let what = if matches!(kind, Kind::Module) { "a module-level block, which is compiled but not run" } else { "a fragment" };
                if let Some((n, _)) = &b.output {
                    return Err(format!("README.md line {n}: this ```text block follows {what}, whose output trantor does not check. State values in `# comments`, move it after a whole app, or fence it as something other than text"));
                }
                if b.exit.is_some() {
                    return Err(format!("README.md line {}: `exit=` applies only to a whole app, and this block is {what}", b.first_line - 1));
                }
                if matches!(kind, Kind::Module) { modules.push(b) } else { fragments.push(b) }
            }
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
        let got = run_source(steps, with, &g.app, "the README.md examples", 0, &readme)?;
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
    // Definitions only, a function or a type among them. A block that also
    // evaluates something — `double(3)   # 6` after `double = |x| ...` — is a
    // fragment with a local helper.
    let top: Vec<(usize, String)> = b.lines.iter().filter(|(_, l)| import_of(l).is_none()).cloned().collect();
    let stmts = statements(&top).0;
    let defines = stmts.iter().any(|st| {
        let first = st.code.lines().next().unwrap_or("");
        let function = binding(st).is_some_and(|(_, rhs)| rhs.trim_start().starts_with('|')) && !first.starts_with(char::is_whitespace);
        let type_decl = first.split_once(" :").is_some_and(|(n, r)| n.starts_with(|c: char| c.is_ascii_uppercase()) && (r.starts_with(':') || r.starts_with('=')));
        function || type_decl
    });
    let evaluates = stmts.iter().any(|st| {
        let declares = st.blanked.contains("::") || st.blanked.contains(":=") || top_level(&st.blanked, ':').is_some();
        binding(st).is_none() && !declares && !st.code.trim_start().starts_with("expect")
    });
    if defines && !evaluates { Kind::Module } else { Kind::Fragment }
}

/// Build and run a whole app; true when it stated its output.
fn whole_app(steps: &Steps, with: &Path, app: &Block) -> Result<bool, String> {
    for (n, raw) in &app.lines {
        if let Some(at) = crate::readme_lex::scan(raw).comment_at {
            let c = raw[at..].trim_start_matches('#').trim().to_string();
            if claim_at(!raw[..at].trim().is_empty(), &c) != Claim::Prose {
                return Err(format!("README.md line {n}: `# {c}` states a value inside a whole app; state its output in a ```text block after the app instead"));
            }
        }
    }
    let source = retarget(&app.lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n"));
    let what = format!("the README.md app at line {}", app.first_line);
    let readme = std::fs::read_to_string(steps.root.join("README.md")).unwrap_or_default();
    let got = run_source(steps, with, &source, &what, app.exit.unwrap_or(0), &readme)?;
    if let Some((n, stated)) = &app.output {
        if &got != stated {
            let (stated, got) = if stated.trim_end() == got.trim_end() { (format!("{stated:?}"), format!("{got:?}")) } else { (stated.clone(), got) };
            return Err(format!("{what} printed something other than the output stated at line {n}\n  stated:\n{}\n  actual:\n{}", indent(&stated), indent(&got)));
        }
    }
    Ok(app.output.is_some())
}

/// Build and run `source` as the scratch world's app, requiring `exit`.
fn run_source(steps: &Steps, with: &Path, source: &str, what: &str, exit: i32, readme: &str) -> Result<String, String> {
    std::fs::write(with.join("app/main.roc"), source).map_err(|e| format!("write {what}: {e}"))?;
    let build = steps.trantor_ran(&["build", s(with), "--app", "app", "--out", "readme"])?;
    if !build.ok() {
        let said = format!("{}{}", build.stdout, build.stderr);
        return Err(format!("{what} does not build{}\n{}", readme_lines(source, &said, readme), build.failure("trantor build")));
    }
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

/// The README lines a build failure's `app/main.roc:L` positions come from,
/// read back through the generated app's `# README.md line N` comments.
/// The app's own lines, not the platform's `platform/main.roc`; a generated
/// line with no README origin maps to nothing. Each README line is quoted, so
/// the error's rewritten code (`r25 = n + 100`) can be matched to the text.
fn readme_lines(source: &str, said: &str, readme: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let text: Vec<&str> = readme.lines().collect();
    let mut found: Vec<usize> = said.match_indices("app/main.roc:").filter_map(|(i, m)| {
        let rest = &said[i + m.len()..];
        rest.split(|c: char| !c.is_ascii_digit()).next()?.parse::<usize>().ok()
    }).filter_map(|generated| {
        let upto = lines.get(..generated.checked_sub(1)?)?;
        let (at, tag) = upto.iter().enumerate().rev().find_map(|(i, l)| {
            let l = l.trim();
            if l == GEN_TAG { Some((i, None)) } else { l.strip_prefix(LINE_TAG).map(|n| (i, Some(n))) }
        })?;
        Some(tag?.trim().parse::<usize>().ok()? + upto.len() - at - 1)
    }).collect();
    found.sort();
    found.dedup();
    found.iter().map(|n| format!("\n  README.md line {n}: {}", text.get(n - 1).map_or("", |l| l.trim()))).collect()
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

/// Compare the generated app's marked output against the claims; the count.
/// Each value sits between its marker and an end marker, so one that spans
/// lines — `Str.inspect` leaves a newline raw — is read whole.
fn compare(expected: &BTreeMap<usize, (Claim, bool)>, stdout: &str) -> Result<usize, String> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{MARK_ERR}:")) {
            let (n, e) = rest.split_once(':').unwrap_or((rest, ""));
            return Err(format!("README.md line {n}: the example returned an error: {e}"));
        }
    }
    let mut got: BTreeMap<usize, &str> = BTreeMap::new();
    for (i, _) in stdout.match_indices(&format!("{MARK}:")) {
        let rest = &stdout[i + MARK.len() + 1..];
        let Some((n, value)) = rest.split_once(':') else { continue };
        let (Ok(n), Some(end)) = (n.parse(), value.find(MARK_END)) else { continue };
        got.insert(n, &value[..end]);
    }
    let mut wrong = vec![];
    for (n, (cl, via)) in expected {
        let actual = got.get(n).ok_or_else(|| format!("README.md line {n}: the example printed no value for its stated one"))?;
        if !matches(cl, *via, actual)? {
            let stated = match cl { Claim::Quoted(t) => format!("{t:?}"), Claim::Token(t) | Claim::Tag(t) => t.clone(), _ => String::new() };
            wrong.push(format!("  README.md line {n}: stated {stated}, actual {actual}"));
        }
    }
    if !wrong.is_empty() {
        return Err(format!("README.md says one thing and the package does another\n{}", wrong.join("\n")));
    }
    Ok(expected.len())
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_app_header_is_retargeted_even_across_lines() {
        let src = "app [main!] {\n\tpf: platform \"../target/trantor/myapp/platform/main.roc\",\n}\nmain! = |_| Ok({})";
        assert!(retarget(src).contains("platform \"../target/trantor/app/platform/main.roc\","));
    }

    #[test]
    fn a_value_spanning_lines_is_read_whole_and_compared() {
        let expected: BTreeMap<usize, (Claim, bool)> = [(7, (Claim::Quoted("2026\n03".into()), false))].into();
        let out = format!("noise\n{MARK}:7:\"2026\n03\"{MARK_END}\nmore\n");
        assert_eq!(compare(&expected, &out), Ok(1));
    }

    #[test]
    fn a_local_helper_with_an_evaluated_line_is_a_fragment_and_definitions_alone_a_module() {
        assert!(matches!(kind(&Block::for_test(&["double = |x| x * 2", "double(3)   # 6"])), Kind::Fragment));
        assert!(matches!(kind(&Block::for_test(&["double = |x| x * 2"])), Kind::Module));
        assert!(matches!(kind(&Block::for_test(&["Greet :: [].{", "\thello = |n| n", "}"])), Kind::Module));
    }

    #[test]
    fn a_build_error_in_the_generated_app_names_the_readme_line() {
        let source = format!("app\n{LINE_TAG}2\nx = 1\ny = oops\n{GEN_TAG}\nmain! = 1\n");
        let readme = "# t\nx = 1\ny = oops\n";
        assert_eq!(readme_lines(&source, "── error ─ app/main.roc:4:5", readme), "\n  README.md line 3: y = oops");
        assert_eq!(readme_lines(&source, "app/main.roc:6:1 and platform/main.roc:4:1", readme), "", "generated and platform lines map to nothing");
    }
}
