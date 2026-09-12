//! README.md's Roc examples, proven (D-T3-2). Every ```roc block compiles and
//! runs; an expression line whose comment opens with a literal value
//! (`# 2024-02-29`, `# "text"`) is compared against what it renders to. Prose
//! comments are exercised, not compared, and a block that reads the clock
//! (`Now.`) is only run. The app is generated from the README so the check
//! cannot drift from what a reader sees.
use std::path::Path;

use crate::package_test::{platform_dir, s, trantor_output, Steps, APP_WORLD};

pub fn check(steps: &Steps, with: &Path) -> Result<(), String> {
    let Ok(readme) = std::fs::read_to_string(steps.root.join("README.md")) else { return Ok(()) };
    let all = blocks(&readme);
    // A block that is a whole app is built and run as written; the rest are
    // fragments, stitched into one generated app.
    let (apps, fragments): (Vec<_>, Vec<_>) = all.into_iter().partition(|b| is_whole_app(b));
    for app in &apps {
        let source = retarget(&app.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n"));
        run_app(with, &source, &format!("the README.md app at line {}", app.first().map_or(0, |l| l.0)))?;
    }
    let mut stated = 0;
    if !fragments.is_empty() {
        let prelude = std::fs::read_to_string(steps.root.join("tests/readme-prelude.roc")).unwrap_or_default();
        let platform = std::fs::read_to_string(platform_dir(with, APP_WORLD).join("main.roc"))
            .map_err(|e| format!("read the composed platform: {e}"))?;
        let generated = generate(&fragments, &bindings(&prelude), &platform)?;
        let got = run_app(with, &generated.app, "a README.md example")?;
        if got != generated.expected {
            return Err(format!("README.md says one thing and the package does another\n  stated:\n{}\n  actual:\n{}",
                indent(&generated.expected), indent(&got)));
        }
        stated = generated.expected.lines().count();
    }
    if apps.len() + fragments.len() > 0 {
        println!("ok: README.md — {} blocks compile and run ({} whole apps), {stated} stated values match",
            apps.len() + fragments.len(), apps.len());
    }
    Ok(())
}

/// Build and run `source` as the scratch world's app; its stdout.
fn run_app(with: &Path, source: &str, what: &str) -> Result<String, String> {
    std::fs::write(with.join("app/main.roc"), source).map_err(|e| format!("write {what}: {e}"))?;
    let out = trantor_output(&["run", s(with)], with)?;
    if !out.status.success() {
        return Err(format!("{what} does not build or run:\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn is_whole_app(block: &[(usize, String)]) -> bool {
    block.iter().map(|(_, l)| l.trim()).find(|l| !l.is_empty() && !l.starts_with('#')).is_some_and(|l| l.starts_with("app ["))
}

/// A README app names its reader's world; point it at the test world instead.
pub fn retarget(source: &str) -> String {
    let mut out = String::new();
    for (i, line) in source.lines().enumerate() {
        match (i, line.split_once("platform \"")) {
            (_, Some((head, rest))) if line.trim_start().starts_with("app [") => {
                let tail = rest.split_once('"').map_or("", |(_, t)| t);
                out.push_str(&format!("{head}platform \"../target/trantor/{APP_WORLD}/platform/main.roc\"{tail}\n"));
            }
            _ => out.push_str(&format!("{line}\n")),
        }
    }
    out
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}

pub struct Generated {
    pub app: String,
    pub expected: String,
}

/// `(line number, text)` for each line of each ```roc block.
pub fn blocks(readme: &str) -> Vec<Vec<(usize, String)>> {
    let (mut out, mut cur): (Vec<Vec<(usize, String)>>, Option<Vec<(usize, String)>>) = (vec![], None);
    for (i, line) in readme.lines().enumerate() {
        if line.starts_with("```roc") {
            cur = Some(vec![]);
        } else if line.starts_with("```") {
            out.extend(cur.take());
        } else if let Some(c) = cur.as_mut() {
            c.push((i + 1, line.to_string()));
        }
    }
    out
}

/// `(code, comment)` split at the first `#` outside a string literal.
pub fn split_comment(line: &str) -> (&str, Option<&str>) {
    let mut quoted = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
            quoted = !quoted;
        } else if b == b'#' && !quoted {
            return (line[..i].trim_end(), Some(line[i + 1..].trim()));
        }
    }
    (line.trim_end(), None)
}

/// The value a comment states, or `None` when it is prose.
pub fn claimed_value(comment: &str) -> Option<String> {
    if let Some(rest) = comment.strip_prefix('"') {
        return rest.find('"').map(|end| rest[..end].to_string());
    }
    let head = comment.split([',']).next()?.split(" —").next()?.trim();
    let literal = head.starts_with(|c: char| c.is_ascii_digit() || c == '-')
        && head.chars().all(|c| c.is_alphanumeric() || ":+-[]/._".contains(c))
        && head.chars().any(|c| c.is_ascii_digit());
    literal.then(|| head.to_string())
}

/// `name = expr` lines, by name, in order.
pub fn bindings(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let code = split_comment(l).0;
            let (name, _) = code.split_once(" = ")?;
            is_ident(name).then(|| (name.to_string(), code.to_string()))
        })
        .collect()
}

/// Whether an expression's last call is one that already yields a `Str`:
/// `to_str`, or `format`, effectful or not.
pub fn renders_str(code: &str) -> bool {
    let code = code.strip_suffix('?').unwrap_or(code);
    let Some(inner) = code.strip_suffix(')') else { return false };
    let mut depth = 1;
    let open = inner.char_indices().rev().find_map(|(i, c)| {
        match c {
            ')' => depth += 1,
            '(' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(i)
    });
    open.is_some_and(|i| [".to_str", ".to_str!", ".format", ".format!"].iter().any(|m| inner[..i].ends_with(m)))
}

fn is_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Whether `name` is used as a variable: not part of a longer identifier, and
/// not a field after a single `.` — but a spread `..name` does use it.
fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let head = &text[..i];
        let after = text[i + name.len()..].chars().next();
        let in_word = head.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_');
        let field = head.ends_with('.') && !head.ends_with("..");
        !in_word && !field && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

pub fn generate(blocks: &[Vec<(usize, String)>], prelude: &[(String, String)], platform: &str) -> Result<Generated, String> {
    let contract = crate::scaffold::main_contract(platform)?;
    let requires = contract.signature.clone();
    let mut imports: Vec<String> = vec!["pf.Stdout".into()];
    imports.extend(contract.imports.iter().cloned());
    let (mut top, mut fns, mut calls, mut expected) = (vec![], vec![], vec![], String::new());
    for (index, block) in blocks.iter().enumerate() {
        imports.extend(block.iter().filter_map(|(_, l)| l.strip_prefix("import ").map(|m| m.trim().to_string())));
        let body: Vec<&(usize, String)> = block.iter().filter(|(_, l)| !l.starts_with("import ")).collect();
        if body.iter().any(|(_, l)| l.split_once(" :").is_some_and(|(n, _)| is_ident(n.trim_end_matches('!')))) {
            top.push(format!("# README.md line {}", block[0].0));
            top.extend(body.iter().map(|(_, l)| l.clone()));
            continue;
        }
        let text: String = body.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");
        let clocked = text.contains("Now.");
        let defined: Vec<&str> = body.iter().filter_map(|(_, l)| split_comment(l).0.split_once(" = ").map(|(n, _)| n)).filter(|n| is_ident(n)).collect();
        // A block that binds nothing is a list of independent calls, which may
        // mix error types no single `?` can carry, so each line stands alone.
        let units: Vec<Vec<&(usize, String)>> = if defined.is_empty() {
            body.iter().filter(|(_, l)| !split_comment(l).0.trim().is_empty()).map(|l| vec![*l]).collect()
        } else {
            vec![body.clone()]
        };
        for (part, unit) in units.iter().enumerate() {
            let unit_text: String = unit.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");
            let mut stmts: Vec<String> = prelude.iter()
                .filter(|(n, _)| !defined.contains(&n.as_str()) && mentions(&unit_text, n))
                .map(|(_, b)| b.clone()).collect();
            for (n, raw) in unit {
                let (code, comment) = split_comment(raw);
                if code.trim().is_empty() {
                    continue;
                }
                if code.split_once(" = ").is_some_and(|(name, _)| is_ident(name)) {
                    stmts.push(code.to_string());
                    continue;
                }
                let value = if clocked { None } else { comment.and_then(claimed_value) };
                let var = if value.is_some() { format!("r{n}") } else { format!("_r{n}") };
                stmts.push(format!("{var} = {code}"));
                if let Some(v) = value {
                    let shown = if renders_str(code) { var.clone() } else { format!("{var}.to_str()") };
                    stmts.push(format!("Stdout.line!(\"line {n}: ${{{shown}}}\") ?? {{}}"));
                    expected.push_str(&format!("line {n}: {v}\n"));
                }
            }
            let (first, name) = (unit[0].0, format!("block_{index}_{part}!"));
            fns.push(format!("# README.md line {first}\n{name} : {{}} => Try({{}}, _)\n{name} = |{{}}| {{\n{}\tOk({{}})\n}}\n",
                stmts.iter().map(|st| format!("\t{st}\n")).collect::<String>()));
            calls.push(format!("\tmatch {name}({{}}) {{ Ok(_) => {{}}, Err(_) => Stdout.line!(\"README.md line {first}: the example returned an error\") ?? {{}} }}"));
        }
    }
    imports.sort();
    imports.dedup();
    let arg = contract.param;
    let app = format!(
        "app [main!] {{ pf: platform \"../target/trantor/{APP_WORLD}/platform/main.roc\" }}\n\n{}\n\n{}\n\n{}\n{requires}\nmain! = |{arg}| {{\n{}\n\tOk({{}})\n}}\n",
        imports.iter().map(|i| format!("import {i}")).collect::<Vec<_>>().join("\n"),
        top.join("\n"), fns.join("\n"), calls.join("\n"));
    Ok(Generated { app, expected })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_inside_a_string_does_not_start_a_comment() {
        assert_eq!(split_comment(r#"x.format("%A #1")   # "Sunday #1""#), (r#"x.format("%A #1")"#, Some(r#""Sunday #1""#)));
    }

    #[test]
    fn a_quoted_comment_states_the_quoted_text() {
        assert_eq!(claimed_value(r#""Sunday  8 March 2026""#).as_deref(), Some("Sunday  8 March 2026"));
    }

    #[test]
    fn a_literal_before_a_comma_or_dash_is_a_stated_value() {
        assert_eq!(claimed_value("2024-02-29, constrained").as_deref(), Some("2024-02-29"));
        assert_eq!(claimed_value("3 — Wednesday").as_deref(), Some("3"));
        assert_eq!(claimed_value("2026-03-08T10:40:00-04:00[America/New_York]").as_deref(), Some("2026-03-08T10:40:00-04:00[America/New_York]"));
    }

    #[test]
    fn only_a_final_to_str_or_format_call_already_renders() {
        assert!(renders_str("jan31.to_str()"));
        assert!(renders_str(r#"march8.format("%A %e")"#));
        assert!(renders_str("after.to_str!()?"));
        assert!(renders_str(r#"z.format!("%H")?"#));
        assert!(!renders_str("jan31.add!({ months: 1 })?"));
        assert!(!renders_str("f(x.to_str())"));
        assert!(!renders_str("jan31.year"));
    }

    #[test]
    fn prose_is_not_a_stated_value() {
        assert_eq!(claimed_value("359 days"), None);
        assert_eq!(claimed_value("a time needs no date"), None);
        assert_eq!(claimed_value("nanoseconds since the epoch"), None);
    }

    #[test]
    fn a_block_gets_only_the_prelude_bindings_it_uses_and_does_not_define() {
        let prelude = bindings("jan31 = D.a\nny = Z.b\n");
        let blocks = blocks("```roc\njan31 = D.c\njan31.x()   # 1\n```\n```roc\nny.y()   # 2\n```\n");
        let g = generate(&blocks, &prelude, "\trequires {\n\t\tmain! : {} => Try({}, _)\n\t}\n\texposes [Stdout,]").unwrap();
        assert!(!g.app.contains("jan31 = D.a"), "a block's own binding must not be shadowed by the prelude");
        assert!(g.app.contains("ny = Z.b"), "a used, undefined name gets the prelude binding");
        assert_eq!(g.expected, "line 3: 1\nline 6: 2\n");
    }

    #[test]
    fn a_block_opening_with_an_app_header_is_a_whole_app() {
        let b = blocks("```roc\n# the whole thing\napp [main!] { pf: platform \"../target/trantor/myapp/platform/main.roc\" }\nmain! = |_| Ok({})\n```\n```roc\nx.y()   # 1\n```\n");
        assert!(is_whole_app(&b[0]) && !is_whole_app(&b[1]));
        assert_eq!(
            retarget("app [main!] { pf: platform \"../target/trantor/myapp/platform/main.roc\" }\nimport pf.Cli"),
            "app [main!] { pf: platform \"../target/trantor/app/platform/main.roc\" }\nimport pf.Cli\n"
        );
    }

    #[test]
    fn a_spread_uses_the_name_and_a_field_does_not() {
        assert!(mentions("{ ..jan31, day: 1 }", "jan31"));
        assert!(!mentions("d.jan31", "jan31"));
        assert!(!mentions("jan31x", "jan31"));
    }

    #[test]
    fn a_binding_free_block_compiles_one_line_at_a_time() {
        let blocks = blocks("```roc\nNow.a!({})?\nNow.b!({})?\n```\n");
        let g = generate(&blocks, &[], "main! : {} => Try({}, _)").unwrap();
        assert!(g.app.contains("block_0_0!") && g.app.contains("block_0_1!"));
        assert!(g.expected.is_empty(), "a clock-reading block states nothing checkable");
    }
}
