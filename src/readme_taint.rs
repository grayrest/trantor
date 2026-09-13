//! What in a README example reads the machine or the moment, so its value
//! cannot be stated (T3b).
use std::collections::BTreeSet;

use crate::readme_generate::{binding, is_ident};
use crate::readme_lex::{scan, Block, Stmt};

/// Modules whose results depend on the machine or the moment: a value read
/// through one differs between runs and machines, so it cannot be stated.
/// Reached through an alias, an `exposing` list, a module-level function or a
/// prelude binding just the same.
const MACHINE_MODULES: &[&str] = &[
    "Now", "Utc", "Clocks", "Env", "Random", "Locale", "Cli", "Stdin", "Streams", "Tty",
    "File", "Fs", "Path", "OsPath", "StrPath", "Cmd", "Subprocess", "Tcp", "Udp", "Http",
];

/// What reads the machine: `Module.` prefixes (aliases included) and names.
pub struct Taint {
    prefixes: BTreeSet<String>,
    names: BTreeSet<String>,
}

impl Taint {
    pub fn of(imports: &BTreeSet<String>, modules: &[&Block], prelude: &[Stmt]) -> Taint {
        let mut t = Taint { prefixes: BTreeSet::new(), names: BTreeSet::new() };
        for spec in imports {
            let (path, rest) = spec.split_once(char::is_whitespace).unwrap_or((spec, ""));
            let module = path.rsplit('.').next().unwrap_or(path);
            if !MACHINE_MODULES.contains(&module) { continue }
            let alias = rest.split_once("as ").map(|(_, a)| a.split_whitespace().next().unwrap_or(module)).unwrap_or(module);
            t.prefixes.extend([format!("{module}."), format!("{alias}.")]);
            if let Some((_, list)) = rest.split_once("exposing") {
                t.names.extend(list.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '!')).filter(|w| is_ident(w.trim_end_matches('!'))).map(str::to_string));
            }
        }
        for name in MACHINE_MODULES {
            t.prefixes.insert(format!("{name}."));
        }
        // A module-level function or a prelude binding that reads the machine
        // makes its name a reader too, however deep the chain.
        let defs: Vec<(String, String)> = modules.iter().flat_map(|b| definitions(b)).chain(
            prelude.iter().filter_map(|p| binding(p).map(|(names, _)| (names.join(" "), p.blanked.clone())))
        ).collect();
        loop {
            let before = t.names.len() + t.prefixes.len();
            for (names, body) in &defs {
                if t.reads(body) {
                    for n in names.split(' ').filter(|n| !n.is_empty()) {
                        // A type module's method is reached as `Type.method`.
                        if n.contains('.') { t.prefixes.insert(n.to_string()); } else { t.names.insert(n.to_string()); }
                    }
                }
            }
            if t.names.len() + t.prefixes.len() == before { break }
        }
        t
    }

    pub fn reads(&self, blanked: &str) -> bool {
        let prefixed = self.prefixes.iter().any(|p| blanked.match_indices(p.as_str()).any(|(i, _)| {
            !blanked[..i].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
        }));
        prefixed || self.names.iter().any(|n| uses(blanked, n))
    }

    /// Statements that read the machine, directly or through an earlier binding.
    pub fn dependent(&self, stmts: &[Stmt], bound: &[Vec<String>]) -> Vec<bool> {
        let mut local: BTreeSet<String> = BTreeSet::new();
        stmts.iter().zip(bound).map(|(st, names)| {
            let reads = self.reads(&st.blanked) || local.iter().any(|t| uses(&st.blanked, t));
            if reads { local.extend(names.iter().cloned()); }
            reads
        }).collect()
    }
}

/// A module-level block's definitions, `(name, its text)`: column-0
/// functions and values, and a type module's methods as `Type.method`. Any
/// other column-0 line — an annotation — starts an unnamed chunk, so it is
/// not read as part of the definition above it.
fn definitions(b: &Block) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    let mut type_module: Option<String> = None;
    for (_, l) in &b.lines {
        let blanked = scan(l).blanked;
        let top = !l.starts_with(char::is_whitespace) && !l.trim().is_empty();
        if top {
            type_module = l.split_once(" :").filter(|(n, r)| n.starts_with(|c: char| c.is_ascii_uppercase()) && (r.starts_with(':') || r.starts_with('='))).map(|(n, _)| n.to_string());
        }
        let lhs = l.split_once(" = ").map(|(lhs, _)| lhs.trim()).filter(|lhs| is_ident(lhs.trim_end_matches('!')));
        match (top, lhs, &type_module) {
            (true, Some(name), _) => out.push((name.to_string(), blanked)),
            (false, Some(name), Some(ty)) => out.push((format!("{ty}.{name}"), blanked)),
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
        let typed = Block::for_test(&["Machine :: [].{", "\thome! = |{}|", "\t\tEnv.var_str!(\"HOME\")", "}", "greet = |n| n", "run : Cmd.Cmd => Try({}, _)"]);
        let t = Taint::of(&BTreeSet::new(), &[&typed], &[]);
        assert!(t.reads(&scan("Machine.home!({})?").blanked), "a type module's method");
        assert!(!t.reads(&scan("greet(\"x\")").blanked), "an annotation below is not part of greet");
        assert!(!t.reads(&scan("MyNow.x").blanked), "a longer module name is not Now");
    }
    #[test]
    fn a_spread_is_a_use_and_a_field_or_longer_name_is_not() {
        assert!(uses("{ ..jan31, day: 1 }", "jan31"));
        assert!(!uses("d.jan31", "jan31") && !uses("jan31x", "jan31"));
        assert!(!uses(&scan(r##"Greet.hello("who")"##).blanked, "who"));
    }
}
