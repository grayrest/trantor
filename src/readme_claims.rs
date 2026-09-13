//! What a README comment claims about a value, how the value is shown so the
//! claim can be compared, and the comparison (T3b). A comment that looks like a
//! value but is not a form listed here fails the run: a claim silently skipped
//! reads the same as one checked.

/// What a comment claims.
#[derive(Debug, PartialEq)]
pub enum Claim {
    Prose,
    /// `"text"` — the expression renders to exactly this.
    Quoted(String),
    /// A single token: a number (`3`, `-1.5`), a Bool (`True`), a tag
    /// (`LT`, `Iso`), or text a value renders as (`2024-02-29`, `-P1D`).
    Token(String),
    /// `Name(...)` — `Str.inspect` of the value is exactly this (`Ok(3)`,
    /// `Err(BadPattern("%Q"))`, `Some(x)`).
    Tag(String),
    /// Starts like a value but is none of the above.
    Unrecognised(String),
}

/// Anything after a top-level ` — ` is prose; what comes before it is
/// classified. A ` — ` inside quotes or brackets belongs to the value.
pub fn claim(comment: &str) -> Claim {
    let head = before_prose(comment).trim();
    if let Some(rest) = head.strip_prefix('"') {
        return match unquote(rest) {
            Some((text, tail)) if tail.trim().is_empty() => Claim::Quoted(text),
            _ => Claim::Unrecognised(head.to_string()),
        };
    }
    let first = head.split_whitespace().next().unwrap_or("");
    let one_token = !head.contains(char::is_whitespace);
    if is_tag(head) {
        return Claim::Tag(head.to_string());
    }
    if one_token && (is_number(head) || is_bool(head) || is_tag_name(head) || is_rendered_text(head)) {
        return Claim::Token(head.to_string());
    }
    if looks_like_value(first) || (one_token && head.starts_with(|c: char| c.is_ascii_uppercase())) {
        return Claim::Unrecognised(head.to_string());
    }
    Claim::Prose
}

fn before_prose(comment: &str) -> &str {
    let (mut depth, mut quoted, mut escaped) = (0i32, false, false);
    for (i, c) in comment.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '(' | '[' | '{' if !quoted => depth += 1,
            ')' | ']' | '}' if !quoted => depth -= 1,
            _ if !quoted && depth == 0 && comment[i..].starts_with(" — ") => return &comment[..i],
            _ => {}
        }
    }
    comment
}

fn is_number(t: &str) -> bool {
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    body.starts_with(|c: char| c.is_ascii_digit()) && t.parse::<f64>().is_ok()
}

fn is_bool(t: &str) -> bool {
    matches!(t, "True" | "False" | "true" | "false")
}

/// `LT`, `Iso`, `Wednesday`: a tag name alone — not a duration like `P359D`.
fn is_tag_name(t: &str) -> bool {
    !is_rendered_text(t) && t.starts_with(|c: char| c.is_ascii_uppercase()) && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `Name(...)`, balanced.
fn is_tag(t: &str) -> bool {
    t.split_once('(').is_some_and(|(name, _)| is_tag_name(name)) && t.ends_with(')') && balanced(t)
}

/// Text a value's `to_str` gives: a date, a duration, an offset.
fn is_rendered_text(t: &str) -> bool {
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    let chars_ok = t.chars().all(|c| c.is_ascii_alphanumeric() || ":.-+[]/_".contains(c));
    chars_ok && body.starts_with(|c: char| c.is_ascii_digit())
        || chars_ok && body.strip_prefix('P').is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit() || c == 'T'))
}

fn looks_like_value(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(c) = chars.next() else { return false };
    let next = chars.next();
    c.is_numeric()
        || "\"'[{(→=".contains(c)
        || (matches!(c, '-' | '+' | '.') && next.is_some_and(|n| n.is_ascii_digit() || n == 'P'))
        || (c == 'P' && next.is_some_and(|n| n.is_ascii_digit() || n == 'T'))
        || is_bool(word.trim_end_matches(|c: char| c.is_ascii_punctuation()))
        || word.starts_with("Bool.")
}

/// A Roc string's contents up to its closing quote, unescaped, and what follows.
pub fn unquote(after_open: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut it = after_open.char_indices();
    while let Some((i, c)) = it.next() {
        match c {
            '"' => return Some((out, &after_open[i + 1..])),
            '\\' => match it.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn balanced(s: &str) -> bool {
    let (mut depth, mut quoted, mut prev) = (0i32, false, ' ');
    for c in s.chars() {
        if c == '"' && prev != '\\' { quoted = !quoted }
        if !quoted {
            match c { '(' => depth += 1, ')' => depth -= 1, _ => {} }
            if depth < 0 { return false }
        }
        prev = c;
    }
    depth == 0 && !quoted
}

/// Whether a claimed value is shown through its `to_str()`: when the claim
/// is text the value renders as and the expression does not already yield a
/// `Str`. Otherwise `Str.inspect` of the value itself is compared.
pub fn via_to_str(claim: &Claim, code: &str) -> bool {
    let text = match claim {
        Claim::Quoted(_) => true,
        Claim::Token(t) => !(is_number(t) || is_bool(t) || is_tag_name(t)),
        _ => false,
    };
    text && !renders_str(code)
}

pub fn shown(var: &str, via_to_str: bool) -> String {
    if via_to_str { format!("Str.inspect({var}.to_str())") } else { format!("Str.inspect({var})") }
}

/// Whether a claim matches `Str.inspect`'s output for its value. A `to_str`
/// trantor called may answer `Ok("...")` — a duration's does — and its text is
/// the claim; an expression the README wrote as a `Str` that is really a
/// `Try` is not.
pub fn matches(claim: &Claim, via_to_str: bool, inspected: &str) -> Result<bool, String> {
    let inspected = match inspected.strip_prefix("Ok(").and_then(|r| r.strip_suffix(')')) {
        Some(inner) if via_to_str && inner.starts_with('"') => inner,
        _ => inspected,
    };
    Ok(match claim {
        Claim::Tag(t) => t == inspected,
        Claim::Token(t) if is_number(t) => inspected.parse::<f64>().is_ok_and(|v| t.parse::<f64>().is_ok_and(|s| s == v)),
        Claim::Token(t) if is_bool(t) => t.eq_ignore_ascii_case(inspected),
        Claim::Token(t) if is_tag_name(t) => t == inspected,
        Claim::Quoted(t) | Claim::Token(t) => as_text(inspected).is_some_and(|s| &s == t),
        Claim::Prose | Claim::Unrecognised(_) => return Err("not a claim".into()),
    })
}

/// A `Str.inspect` of a Str, as the plain text. An `Ok("...")` is not one: a
/// `Try` stated as its text used to pass without the `?` it needs.
fn as_text(inspected: &str) -> Option<String> {
    inspected.strip_prefix('"').and_then(unquote).filter(|(_, tail)| tail.is_empty()).map(|(t, _)| t)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_are_classified_and_near_misses_are_unrecognised() {
        assert_eq!(claim(r##""hello \"reader\"""##), Claim::Quoted("hello \"reader\"".into()));
        assert_eq!(claim("2024-02-29 — constrained"), Claim::Token("2024-02-29".into()));
        assert_eq!(claim("P359D — 359 days"), Claim::Token("P359D".into()));
        assert_eq!(claim(r##"Err(BadInput("no, it isn't"))"##), Claim::Tag(r##"Err(BadInput("no, it isn't"))"##.into()));
        assert_eq!(claim("a time needs no date"), Claim::Prose);
        assert_eq!(claim("Wednesday is the day it lands on"), Claim::Prose);
        assert!(via_to_str(&Claim::Token("P359D".into()), "a.until!(b)?"), "a duration is text, not a tag");
        for token in ["-15", "3.5", "-P1D", "-PT23H", "+05:00", "LT", "True", "true", "Iso", "Wednesday"] {
            assert_eq!(claim(token), Claim::Token(token.into()), "{token}");
        }
        assert_eq!(claim("OutOfRange(\"x\")"), Claim::Tag("OutOfRange(\"x\")".into()));
        for near in ["2024-02-29, constrained", "359 days", "999 (paren after)", "[1, 2]", r##""x" and more"##, "3,000",
                     "→ 3", "= 3", "=> 3", "'a'", "３", ".5", "Bool.true", "True, as it happens", "\"x\"."] {
            assert!(matches!(claim(near), Claim::Unrecognised(_)), "{near} should be unrecognised, got {:?}", claim(near));
        }
    }

    #[test]
    fn a_dash_inside_a_quoted_or_tagged_value_is_part_of_it() {
        assert_eq!(claim("\"hello a — b\""), Claim::Quoted("hello a — b".into()));
        assert_eq!(claim("Err(BadInput(\"x — y\")) — why"), Claim::Tag("Err(BadInput(\"x — y\"))".into()));
    }

    #[test]
    fn values_compare_by_what_they_are() {
        assert!(matches(&Claim::Token("3".into()), false, "3.0").unwrap(), "an untyped literal is a Dec");
        assert!(!matches(&Claim::Token("3".into()), false, "4").unwrap());
        assert!(matches(&Claim::Token("true".into()), false, "True").unwrap());
        assert!(matches(&Claim::Token("LT".into()), false, "LT").unwrap());
        assert!(matches(&Claim::Quoted("2026\n03".into()), false, "\"2026\n03\"").unwrap(), "inspect leaves a newline raw");
        assert!(!matches(&Claim::Quoted("10".into()), false, "Ok(\"10\")").unwrap(), "a Try the README wrote is not its text");
        assert!(matches(&Claim::Token("P359D".into()), true, "Ok(\"P359D\")").unwrap(), "a to_str trantor called may answer Ok");
        assert!(matches(&Claim::Tag("Ok(1)".into()), false, "Ok(1)").unwrap());
    }

    #[test]
    fn a_number_bool_or_tag_is_inspected_and_text_goes_through_to_str() {
        assert!(!via_to_str(&Claim::Token("3".into()), "x"));
        assert!(!via_to_str(&Claim::Token("True".into()), "a == b"));
        assert!(via_to_str(&Claim::Token("2024-02-29".into()), "d"));
        assert!(!via_to_str(&Claim::Quoted("x".into()), "d.to_str()"));
        assert_eq!(shown("r", true), "Str.inspect(r.to_str())");
    }

    #[test]
    fn only_a_final_to_str_or_format_call_already_renders() {
        assert!(renders_str("jan31.to_str()") && renders_str(r#"march8.format("%A %e")"#) && renders_str(r#"z.format!("%H")?"#));
        assert!(!renders_str("jan31.add!({ months: 1 })?") && !renders_str("f(x.to_str())") && !renders_str("jan31.year"));
        assert!(!renders_str("after.to_str!()"), "to_str! without ? is a Try, not a Str");
    }
}
