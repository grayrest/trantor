//! What in a README example reads the machine or the moment, so its value
//! cannot be stated (T3b).
use std::collections::BTreeSet;

use crate::readme_generate::{binding, is_ident};
use crate::readme_lex::{scan, Block, Stmt};

/// Modules whose effectful calls read the machine or the moment: a value read
/// through one differs between runs and machines, so it cannot be stated. Only
/// `!` calls count — in Roc a pure function cannot read anything — reached
/// through the module, an alias, an `exposing` list, a value built from the
/// module (`p = Path.unix(..)`, then `p.exists!()`), or a helper, prelude
/// binding or type method that makes one (D-T3d). A module the package under
/// test exports is its own, not the baseline's, and is not on the list.
const MACHINE_MODULES: &[&str] = &[
    "Now", "Utc", "Clocks", "Env", "Random", "Locale", "Cli", "Stdin", "Streams", "Tty",
    "File", "Fs", "Path", "OsPath", "StrPath", "Cmd", "Subprocess", "Tcp", "Udp", "Http",
];

/// What reads the machine.
pub struct Taint {
    /// Module names and aliases whose `!` calls read it.
    modules: BTreeSet<String>,
    /// Functions that read it: exposed effectful functions, helpers, prelude
    /// bindings, and `Type.method`s.
    names: BTreeSet<String>,
    /// Values built from a machine module, whose `!` methods read it.
    values: BTreeSet<String>,
}

impl Taint {
    pub fn of(imports: &BTreeSet<String>, own: &BTreeSet<String>, modules: &[&Block], prelude: &[Stmt]) -> Taint {
        let listed: BTreeSet<&str> = MACHINE_MODULES.iter().copied().filter(|m| !own.contains(*m)).collect();
        let mut t = Taint { modules: listed.iter().map(|m| m.to_string()).collect(), names: BTreeSet::new(), values: BTreeSet::new() };
        for spec in imports {
            let (path, rest) = spec.split_once(char::is_whitespace).unwrap_or((spec, ""));
            let module = path.rsplit('.').next().unwrap_or(path);
            if !listed.contains(module) { continue }
            if let Some((_, a)) = rest.split_once("as ") {
                t.modules.insert(a.split_whitespace().next().unwrap_or(module).to_string());
            }
            if let Some((_, list)) = rest.split_once("exposing") {
                t.names.extend(list.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '!')).filter(|w| w.ends_with('!') && is_ident(w.trim_end_matches('!'))).map(str::to_string));
            }
        }
        let defs: Vec<(String, String)> = modules.iter().flat_map(|b| definitions(b)).chain(
            prelude.iter().filter_map(|p| binding(p).map(|(names, _)| (names.join(" "), p.blanked.clone())))
        ).collect();
        for (names, body) in &defs {
            if !names.contains('.') && t.builds_value(body) {
                t.values.extend(names.split(' ').filter(|n| !n.is_empty() && !n.ends_with('!')).map(str::to_string));
            }
        }
        // A helper that reads makes its name a reader too, however deep.
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
        self.modules.iter().any(|m| effectful_after(blanked, &format!("{m}.")))
            || self.values.iter().any(|v| effectful_after(blanked, &format!("{v}.")))
            || self.names.iter().any(|n| if n.contains('.') { effectful_after(blanked, n) || word_at(blanked, n) } else { uses(blanked, n) })
    }

    /// Code that mentions a machine module at all — `Path.unix("a")`, `: Path`.
    fn builds_value(&self, blanked: &str) -> bool {
        self.modules.iter().any(|m| word_at(blanked, m))
    }

    /// Statements that read the machine, directly or through an earlier binding
    /// or value.
    pub fn dependent(&self, stmts: &[Stmt], bound: &[Vec<String>]) -> Vec<bool> {
        let mut local: BTreeSet<String> = BTreeSet::new();
        let mut values = self.values.clone();
        stmts.iter().zip(bound).map(|(st, names)| {
            // `here : Path`, or `p = Path.unix("a")`: a machine value.
            let annotated = names.is_empty().then(|| st.code.split_once(':').map(|(l, _)| l.trim().to_string())).flatten().filter(|l| is_ident(l));
            if self.builds_value(&st.blanked) {
                values.extend(names.iter().cloned().chain(annotated));
            }
            let reads = self.reads(&st.blanked)
                || values.iter().any(|v| effectful_after(&st.blanked, &format!("{v}.")))
                || local.iter().any(|t| uses(&st.blanked, t));
            if reads { local.extend(names.iter().cloned()); }
            reads
        }).collect()
    }
}

/// `prefix` at a word boundary, followed by an identifier ending in `!` — or,
/// for a full `Type.method!` name, the name itself.
fn effectful_after(blanked: &str, prefix: &str) -> bool {
    blanked.match_indices(prefix).any(|(i, _)| {
        if blanked[..i].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            return false;
        }
        if prefix.ends_with('!') {
            return true;
        }
        let rest = &blanked[i + prefix.len()..];
        let ident: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        !ident.is_empty() && rest[ident.len()..].starts_with('!')
    })
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
        let t = Taint::of(&imports, &BTreeSet::new(), &[&module], &prelude);
        for line in ["Clock.time_zone_id!({})?", "machine_zone!({})?", "next_year.to_str()"] {
            assert!(t.reads(&scan(line).blanked), "{line}");
        }
        assert!(!t.reads(&scan("jan1.add!({ days: 1 })?").blanked));
        let typed = Block::for_test(&["Machine :: [].{", "\thome! = |{}|", "\t\tEnv.var_str!(\"HOME\")", "\tos! = || {", "\t\tinfo = Env.platform!()", "\t\tinfo.os", "\t}", "}", "greet = |n| n", "run : Cmd.Cmd => Try({}, _)"]);
        let t = Taint::of(&BTreeSet::new(), &BTreeSet::new(), &[&typed], &[]);
        assert!(t.reads(&scan("Machine.os!()").blanked), "a method with a local binding");
        assert!(t.reads(&scan("Machine.home!({})?").blanked), "a type module's method");
        assert!(!t.reads(&scan("greet(\"x\")").blanked), "an annotation below is not part of greet");
        assert!(!t.reads(&scan("MyNow.x").blanked), "a longer module name is not Now");
    }
    #[test]
    fn only_effectful_calls_read_and_a_package_module_is_its_own() {
        let t = Taint::of(&BTreeSet::new(), &BTreeSet::new(), &[], &[]);
        assert!(!t.reads(&scan("Path.unix(\"a/b\").to_str()").blanked), "a pure call");
        assert!(t.reads(&scan("Env.var!(\"HOME\")").blanked));
        let own: BTreeSet<String> = ["Random".to_string()].into();
        let t = Taint::of(&BTreeSet::new(), &own, &[], &[]);
        assert!(!t.reads(&scan("Random.next!(3)").blanked), "the package's own Random");
        let st = stmts(&["here : Path", "here = \"app/main.roc\"", "here.exists!()?"]);
        let bound: Vec<Vec<String>> = st.iter().map(|s| binding(s).map(|(n, _)| n).unwrap_or_default()).collect();
        assert_eq!(t.dependent(&st, &bound), vec![false, false, true], "a Path value's effectful method");
    }

    #[test]
    fn a_spread_is_a_use_and_a_field_or_longer_name_is_not() {
        assert!(uses("{ ..jan31, day: 1 }", "jan31"));
        assert!(!uses("d.jan31", "jan31") && !uses("jan31x", "jan31"));
        assert!(!uses(&scan(r##"Greet.hello("who")"##).blanked, "who"));
    }
}
