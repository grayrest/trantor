//! What in a README example reads the machine or the moment, so its value
//! cannot be stated (T3b).
use std::collections::BTreeSet;

use crate::readme_generate::{binding, is_ident};
use crate::readme_lex::{scan, Block, Stmt};

/// Modules whose results depend on the machine or the moment: a value read
/// through one differs between runs and machines, so it cannot be stated.
/// Reached through an alias, an `exposing` list, a module-level function or a
/// prelude binding just the same.
const MACHINE_MODULES: &[&str] = &["Now", "Utc", "Clocks", "Env", "Random", "Locale", "Cli", "Stdin", "File", "Fs", "Cmd", "Subprocess"];

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
            let before = t.names.len();
            for (names, body) in &defs {
                if t.reads(body) {
                    t.names.extend(names.split(' ').filter(|n| !n.is_empty()).map(str::to_string));
                }
            }
            if t.names.len() == before { break }
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

/// A module-level block's column-0 definitions: `(name, its text)`.
fn definitions(b: &Block) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for (_, l) in &b.lines {
        let blanked = scan(l).blanked;
        match l.split_once(" = ") {
            Some((lhs, _)) if !l.starts_with(char::is_whitespace) && is_ident(lhs.trim_end_matches('!')) => out.push((lhs.to_string(), blanked)),
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
        assert!(!t.reads(&scan("MyNow.x").blanked), "a longer module name is not Now");
    }
    #[test]
    fn a_spread_is_a_use_and_a_field_or_longer_name_is_not() {
        assert!(uses("{ ..jan31, day: 1 }", "jan31"));
        assert!(!uses("d.jan31", "jan31") && !uses("jan31x", "jan31"));
        assert!(!uses(&scan(r##"Greet.hello("who")"##).blanked, "who"));
    }
}
