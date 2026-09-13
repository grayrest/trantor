//! What in a README example reads the machine or the moment, so its value
//! cannot be stated (T3b).
use std::collections::BTreeSet;

use crate::readme_generate::{binding, is_ident};
use crate::readme_lex::{scan, Block, Stmt};

/// Modules that read the machine or the moment. A README statement cannot state
/// a value when it makes an effectful (`!`) call AND touches one of these —
/// directly, through an alias or `exposing` list, or through a name whose
/// definition touches one (a helper, prelude binding, type method or earlier
/// binding) — or when it uses a value such a statement produced (D-T3-20).
/// How the machine value flows (a chain, a returned value, a lambda argument,
/// a field) does not matter, and a deterministic effectful call on an unlisted
/// module (a date's `add!`) stays stateable. The list applies to a package's
/// own modules too: trantor-temporal ships `Now`, and `Now` reads the clock.
const MACHINE_MODULES: &[&str] = &[
    "Now", "Utc", "Clocks", "Env", "Random", "Locale", "Cli", "Stdin", "Streams", "Tty",
    "File", "Fs", "Path", "OsPath", "StrPath", "Cmd", "Subprocess", "Tcp", "Udp", "Http",
];

/// What reads the machine.
pub struct Taint {
    /// Module names and aliases on the list.
    modules: BTreeSet<String>,
    /// Names whose definition touches a listed module: `home = || Path.unix(..)`.
    touching: BTreeSet<String>,
    /// Names whose value was read from the machine: `year = Now.date!(..)?.year`.
    read: BTreeSet<String>,
}

impl Taint {
    pub fn of(imports: &BTreeSet<String>, modules: &[&Block], prelude: &[Stmt]) -> Taint {
        let mut t = Taint { modules: MACHINE_MODULES.iter().map(|m| m.to_string()).collect(), touching: BTreeSet::new(), read: BTreeSet::new() };
        for spec in imports {
            let (path, rest) = spec.split_once(char::is_whitespace).unwrap_or((spec, ""));
            let module = path.rsplit('.').next().unwrap_or(path);
            if !MACHINE_MODULES.contains(&module) { continue }
            if let Some((_, a)) = rest.split_once("as ") {
                t.modules.insert(a.split_whitespace().next().unwrap_or(module).to_string());
            }
            if let Some((_, list)) = rest.split_once("exposing") {
                t.touching.extend(list.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '!')).filter(|w| is_ident(w.trim_end_matches('!'))).map(str::to_string));
            }
        }
        let defs: Vec<(Vec<String>, String)> = modules.iter().flat_map(|b| definitions(b)).map(|(n, body)| (vec![n], body)).chain(
            prelude.iter().filter_map(|p| binding(p).map(|(names, _)| (names, p.blanked.clone())))
        ).collect();
        loop {
            let before = t.touching.len() + t.read.len();
            for (names, body) in &defs {
                let names = names.iter().filter(|n| !n.is_empty()).cloned();
                if t.reads(body) {
                    t.read.extend(names.clone());
                }
                if t.touches(body) {
                    t.touching.extend(names);
                }
            }
            if t.touching.len() + t.read.len() == before { break }
        }
        t
    }

    /// Code that mentions a listed module, or a name defined by touching one.
    fn touches(&self, blanked: &str) -> bool {
        self.modules.iter().any(|m| word_at(blanked, m)) || self.touching.iter().any(|n| mentions(blanked, n))
    }

    /// Code whose value comes from the machine.
    pub fn reads(&self, blanked: &str) -> bool {
        (effectful(blanked) && self.touches(blanked)) || self.read.iter().any(|n| mentions(blanked, n))
    }

    /// Statements that read the machine, directly or through an earlier binding.
    pub fn dependent(&self, stmts: &[Stmt], bound: &[Vec<String>]) -> Vec<bool> {
        let mut local = Taint { modules: self.modules.clone(), touching: self.touching.clone(), read: self.read.clone() };
        stmts.iter().zip(bound).map(|(st, names)| {
            // `here : Path` touches Path as surely as `here = Path.unix(..)`.
            let annotated = names.is_empty().then(|| st.code.split_once(':').map(|(l, _)| l.trim().to_string())).flatten().filter(|l| is_ident(l));
            let reads = local.reads(&st.blanked);
            if local.touches(&st.blanked) {
                local.touching.extend(names.iter().cloned().chain(annotated));
            }
            if reads {
                local.read.extend(names.iter().cloned());
            }
            reads
        }).collect()
    }
}

/// An effectful call: a name ending in `!` (not `!=`).
fn effectful(blanked: &str) -> bool {
    let b: Vec<char> = blanked.chars().collect();
    b.iter().enumerate().any(|(i, c)| {
        *c == '!' && i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_') && b.get(i + 1) != Some(&'=')
    })
}

/// `name` used in code: a plain name by `uses`, a `Type.method` by word.
fn mentions(blanked: &str, name: &str) -> bool {
    if name.contains('.') { word_at(blanked, name.trim_end_matches('!')) } else { uses(blanked, name) }
}

/// `word` standing alone, not part of a longer name.
fn word_at(blanked: &str, word: &str) -> bool {
    blanked.match_indices(word).any(|(i, _)| {
        let before = blanked[..i].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
        let after = blanked[i + word.len()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_');
        !before && !after
    })
}

/// A module-level block's definitions, `(name, its text)`: column-0
/// functions and values, and a type module's methods as `Type.method` — at the
/// body's own indentation, so a local binding inside a method stays part of
/// it. Any other column-0 line (an annotation) starts an unnamed chunk.
fn definitions(b: &Block) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    let (mut type_module, mut method_indent): (Option<String>, Option<usize>) = (None, None);
    for (_, l) in &b.lines {
        let blanked = scan(l).blanked;
        let indent = l.len() - l.trim_start().len();
        let top = indent == 0 && !l.trim().is_empty();
        if top {
            type_module = l.split_once(" :").filter(|(n, r)| n.starts_with(|c: char| c.is_ascii_uppercase()) && (r.starts_with(':') || r.starts_with('='))).map(|(n, _)| n.to_string());
            method_indent = None;
        }
        let lhs = l.split_once(" = ").map(|(lhs, _)| lhs.trim()).filter(|lhs| is_ident(lhs.trim_end_matches('!')));
        let is_method = !top && type_module.is_some() && lhs.is_some() && method_indent.is_none_or(|m| m == indent);
        match (top, lhs, &type_module) {
            (true, Some(name), _) => out.push((name.to_string(), blanked)),
            (false, Some(name), Some(ty)) if is_method => {
                method_indent = Some(indent);
                out.push((format!("{ty}.{name}"), blanked));
            }
            (true, _, _) => out.push((String::new(), blanked)),
            _ => if let Some(last) = out.last_mut() { last.1.push(' '); last.1.push_str(&blanked) },
        }
    }
    out
}

/// Lowercase identifiers `name` is used as in `blanked` code: not a field
/// after a single `.`, not part of a longer name. `..name` is a use.
pub fn uses(blanked: &str, name: &str) -> bool {
    blanked.match_indices(name).any(|(i, _)| {
        let head = &blanked[..i];
        let after = blanked[i + name.len()..].chars().next();
        let in_word = head.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_');
        let field = head.ends_with('.') && !head.ends_with("..");
        !in_word && !field && !after.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '!')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readme_lex::statements;

    fn stmts(src: &[&str]) -> Vec<Stmt> {
        statements(&src.iter().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect::<Vec<_>>()).0
    }

    #[test]
    fn the_machine_is_read_through_an_alias_a_helper_or_the_prelude() {
        let imports: BTreeSet<String> = ["pf.Now as Clock".to_string()].into();
        let module = Block::for_test(&["machine_zone! = |{}|", "\tNow.time_zone_id!({})"]);
        let prelude = stmts(&["this_year = Now.plain_date_in!(\"UTC\")?.year", "next_year = this_year + 1"]);
        let t = Taint::of(&imports, &[&module], &prelude);
        for line in ["Clock.time_zone_id!({})?", "machine_zone!({})?", "next_year.to_str()"] {
            assert!(t.reads(&scan(line).blanked), "{line}");
        }
        assert!(!t.reads(&scan("jan1.add!({ days: 1 })?").blanked));
        let typed = Block::for_test(&["Machine :: [].{", "\thome! = |{}|", "\t\tEnv.var_str!(\"HOME\")", "\tos! = || {", "\t\tinfo = Env.platform!()", "\t\tinfo.os", "\t}", "}", "greet = |n| n", "run : Cmd.Cmd => Try({}, _)"]);
        let t = Taint::of(&BTreeSet::new(), &[&typed], &[]);
        assert!(t.reads(&scan("Machine.os!()").blanked), "a method with a local binding");
        assert!(t.reads(&scan("Machine.home!({})?").blanked), "a type module's method");
        assert!(!t.reads(&scan("greet(\"x\")").blanked), "an annotation below is not part of greet");
        assert!(!t.reads(&scan("MyNow.x").blanked), "a longer module name is not Now");
    }
    #[test]
    fn an_effectful_statement_touching_a_machine_module_reads_it_however_the_value_flows() {
        let t = Taint::of(&BTreeSet::new(), &[], &[]);
        let dep = |src: &[&str]| {
            let st = stmts(src);
            let bound: Vec<Vec<String>> = st.iter().map(|s| binding(s).map(|(n, _)| n).unwrap_or_default()).collect();
            t.dependent(&st, &bound)
        };
        assert_eq!(dep(&["Path.unix(\"a/b\").to_str()"]), vec![false], "pure");
        assert_eq!(dep(&["Path.unix(\"/usr\").exists!()?"]), vec![true], "a chain");
        assert_eq!(dep(&["home = || Path.unix(\"/usr\")", "home().is_dir!()?"]), vec![false, true], "a returned value");
        assert_eq!(dep(&["check! = |p| p.exists!()", "check!(Path.unix(\"/usr\"))?"]), vec![false, true], "a lambda argument");
        assert_eq!(dep(&["here : Path", "here = \"app/main.roc\"", "here.exists!()?"]), vec![false, false, true], "an annotated value");
        assert_eq!(dep(&["home = Env.var!(\"HOME\")?", "home.count_utf8_bytes()"]), vec![true, true], "a value read, then used purely");
        assert_eq!(dep(&["Now.plain_date_in!(\"UTC\")?.year"]), vec![true], "a package's own Now still reads the clock");
        assert_eq!(dep(&["jan1.add!({ days: 1 })?"]), vec![false], "a deterministic effectful call");
        assert_eq!(dep(&["a != b"]), vec![false], "!= is not a call");
    }

    #[test]
    fn a_spread_is_a_use_and_a_field_or_longer_name_is_not() {
        assert!(uses("{ ..jan31, day: 1 }", "jan31"));
        assert!(!uses("d.jan31", "jan31") && !uses("jan31x", "jan31"));
        assert!(!uses(&scan(r##"Greet.hello("who")"##).blanked, "who"));
    }
}
