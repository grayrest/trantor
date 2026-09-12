//! Reading README.md's Roc examples closely enough to check them (T3b): fenced
//! blocks, statements that span lines, where a comment starts, and what a
//! comment claims.

/// One fenced ```roc block.
pub struct Block {
    /// README line of the first body line.
    pub first_line: usize,
    pub lines: Vec<(usize, String)>,
    /// `exit=N` from the fence's info string; a whole app must exit with it.
    pub exit: i32,
    /// The ```text block that follows it (blank lines between allowed), if
    /// any: a whole app's stated stdout.
    pub output: Option<String>,
    /// 0-based index of its closing fence.
    close: usize,
}

/// Every ```roc (or ~~~roc) block, indented or not. An unterminated fence is
/// an error rather than a block silently dropped.
pub fn blocks(readme: &str) -> Result<Vec<Block>, String> {
    let lines: Vec<&str> = readme.lines().collect();
    let mut out: Vec<Block> = vec![];
    let mut i = 0;
    while i < lines.len() {
        let Some((indent, fence, info)) = fence_open(lines[i]) else { i += 1; continue };
        let start = i;
        let mut body = vec![];
        i += 1;
        while i < lines.len() && !is_fence_close(lines[i], &fence) {
            let l = lines[i];
            let stripped = if l.len() >= indent && l[..indent].trim().is_empty() { &l[indent..] } else { l.trim_start() };
            body.push((i + 1, stripped.to_string()));
            i += 1;
        }
        if i >= lines.len() {
            return Err(format!("README.md line {}: the ``` block opened here is never closed", start + 1));
        }
        i += 1;
        let lang = info.split_whitespace().next().unwrap_or("");
        match lang {
            "roc" => {
                let exit = info.split_whitespace().find_map(|w| w.strip_prefix("exit=")).map_or(Ok(0), |n| {
                    n.parse().map_err(|_| format!("README.md line {}: `exit={n}` is not an exit status", start + 1))
                })?;
                out.push(Block { first_line: start + 2, lines: body, exit, output: None, close: i - 1 });
            }
            "text" | "output" if out.last().is_some_and(|b| b.output.is_none() && lines[b.close + 1..start].iter().all(|l| l.trim().is_empty())) => {
                let text: String = body.iter().map(|(_, l)| format!("{l}\n")).collect();
                if let Some(b) = out.last_mut() {
                    b.output = Some(text);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn fence_open(line: &str) -> Option<(usize, String, String)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    for f in ["```", "~~~"] {
        if let Some(info) = t.strip_prefix(f) {
            return Some((indent, f.to_string(), info.trim().to_string()));
        }
    }
    None
}

fn is_fence_close(line: &str, fence: &str) -> bool {
    line.trim() == fence
}

/// A physical line, scanned: where its comment starts (outside strings, char
/// literals and interpolations), its bracket depth change, and the code with
/// string and char contents blanked, so a name inside a string is not a use.
pub struct Scan {
    pub comment_at: Option<usize>,
    pub depth: i32,
    pub blanked: String,
}

pub fn scan(line: &str) -> Scan {
    #[derive(PartialEq)]
    enum S { Code, Str, Char }
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let (mut state, mut depth, mut interp) = (S::Code, 0i32, Vec::<i32>::new());
    let mut blanked = String::with_capacity(line.len());
    let mut k = 0;
    while k < chars.len() {
        let (at, c) = chars[k];
        match state {
            S::Code => match c {
                '#' => return Scan { comment_at: Some(at), depth, blanked },
                '"' => { state = S::Str; blanked.push('"') }
                '\'' => { state = S::Char; blanked.push('\'') }
                '(' | '[' | '{' => { depth += 1; blanked.push(c) }
                ')' | ']' => { depth -= 1; blanked.push(c) }
                '}' => {
                    if interp.last() == Some(&depth) {
                        interp.pop();
                        state = S::Str;
                        blanked.push(' ');
                    } else {
                        depth -= 1;
                        blanked.push(c);
                    }
                }
                _ => blanked.push(c),
            },
            S::Str => match c {
                '\\' => { blanked.push_str("  "); k += 1 }
                '"' => { state = S::Code; blanked.push('"') }
                '$' if chars.get(k + 1).map(|x| x.1) == Some('{') => {
                    interp.push(depth);
                    state = S::Code;
                    blanked.push_str("  ");
                    k += 1;
                }
                _ => blanked.push(' '),
            },
            S::Char => match c {
                '\\' => { blanked.push_str("  "); k += 1 }
                '\'' => { state = S::Code; blanked.push('\'') }
                _ => blanked.push(' '),
            },
        }
        k += 1;
    }
    Scan { comment_at: None, depth, blanked }
}

/// A statement: one or more physical lines, joined while brackets are open,
/// with the comments that state something about it.
pub struct Stmt {
    pub line: usize,
    pub code: String,
    /// The code with strings blanked, for finding which names it uses.
    pub blanked: String,
    /// Its inline comment, then any comment-only lines directly below it.
    pub comments: Vec<(usize, String)>,
}

/// Statements, and comment-only lines that follow no statement.
pub fn statements(lines: &[(usize, String)]) -> (Vec<Stmt>, Vec<(usize, String)>) {
    let (mut stmts, mut loose): (Vec<Stmt>, Vec<(usize, String)>) = (vec![], vec![]);
    let mut open: Option<Stmt> = None;
    let mut depth = 0;
    let mut last_was_stmt = false;
    for (n, raw) in lines {
        let sc = scan(raw);
        let code = raw[..sc.comment_at.unwrap_or(raw.len())].trim_end();
        let comment = sc.comment_at.map(|at| raw[at..].trim_start_matches('#').trim().to_string());
        let blanked = sc.blanked.trim_end().to_string();
        if let Some(st) = open.as_mut() {
            st.code.push('\n');
            st.code.push_str(code);
            st.blanked.push(' ');
            st.blanked.push_str(&blanked);
            if let Some(c) = comment { st.comments.push((*n, c)); }
            depth += sc.depth;
            if depth <= 0 {
                stmts.extend(open.take());
                last_was_stmt = true;
            }
            continue;
        }
        if code.trim().is_empty() {
            match comment {
                Some(c) if last_was_stmt => stmts.last_mut().map(|s| s.comments.push((*n, c))).unwrap_or(()),
                Some(c) => loose.push((*n, c)),
                None => last_was_stmt = false,
            }
            continue;
        }
        let st = Stmt { line: *n, code: code.to_string(), blanked, comments: comment.map(|c| vec![(*n, c)]).unwrap_or_default() };
        depth = sc.depth;
        if depth > 0 {
            open = Some(st);
        } else {
            stmts.push(st);
            last_was_stmt = true;
        }
    }
    stmts.extend(open.take());
    (stmts, loose)
}

/// What a comment claims.
#[derive(Debug, PartialEq)]
pub enum Claim {
    Prose,
    /// `"text"` — the expression renders to exactly this.
    Quoted(String),
    /// `2024-02-29`, `3`, `P359D`, `true` — renders to exactly this.
    Token(String),
    /// `Ok(...)` / `Err(...)` — `Str.inspect` of the value is exactly this.
    Tag(String),
    /// Starts like a value but is none of the above.
    Unrecognised(String),
}

/// Anything after ` — ` is prose; what comes before it is classified.
pub fn claim(comment: &str) -> Claim {
    let head = comment.split(" — ").next().unwrap_or("").trim();
    if let Some(rest) = head.strip_prefix('"') {
        return match unquote(rest) {
            Some((text, tail)) if tail.trim().is_empty() => Claim::Quoted(text),
            _ => Claim::Unrecognised(head.to_string()),
        };
    }
    if (head.starts_with("Ok(") || head.starts_with("Err(")) && head.ends_with(')') && balanced(head) {
        return Claim::Tag(head.to_string());
    }
    let starts_like_value = head.starts_with(|c: char| c.is_ascii_digit() || "[{(\"".contains(c))
        || head.strip_prefix('-').is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        || head.starts_with("Ok(") || head.starts_with("Err(")
        || head.strip_prefix('P').is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit() || c == 'T'))
        || head == "true" || head == "false"
        || head.split_whitespace().next().is_some_and(|w| w == "true" || w == "false");
    if !starts_like_value {
        return Claim::Prose;
    }
    if !head.contains(char::is_whitespace) && !head.starts_with(['[', '{', '(']) {
        return Claim::Token(head.to_string());
    }
    Claim::Unrecognised(head.to_string())
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

    #[test]
    fn a_hash_in_a_string_char_or_interpolation_is_not_a_comment() {
        // In each, the comment is the `#` after the three spaces.
        for line in [
            r##"x.format("%A #1")   # "Sunday #1""##,
            r##""${Greet.hello("#1")}!"   # "hello #1!""##,
            r"s.contains('#')   # true",
            r##"Greet.hello("a").ends_with("\\")   # "x""##,
        ] {
            assert_eq!(scan(line).comment_at, line.rfind("   #").map(|i| i + 3), "{line}");
        }
    }

    #[test]
    fn a_multi_line_expression_is_one_statement_with_its_comments() {
        let lines: Vec<(usize, String)> = ["long = Greet.hello(", "\t\"multi\",", ")", "long   # \"hello multi\"", "# \"below\""]
            .iter().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect();
        let (stmts, loose) = statements(&lines);
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0].code, "long = Greet.hello(\n\t\"multi\",\n)");
        assert_eq!(stmts[1].comments, vec![(4, "\"hello multi\"".to_string()), (5, "\"below\"".to_string())]);
        assert!(loose.is_empty());
    }

    #[test]
    fn claims_are_classified_and_near_misses_are_unrecognised() {
        assert_eq!(claim(r##""hello \"reader\"""##), Claim::Quoted("hello \"reader\"".into()));
        assert_eq!(claim("2024-02-29 — constrained"), Claim::Token("2024-02-29".into()));
        assert_eq!(claim("P359D — 359 days"), Claim::Token("P359D".into()));
        assert_eq!(claim("-15"), Claim::Token("-15".into()));
        assert_eq!(claim(r##"Err(BadInput("no, it isn't"))"##), Claim::Tag(r##"Err(BadInput("no, it isn't"))"##.into()));
        assert_eq!(claim("a time needs no date"), Claim::Prose);
        // A token is compared whole, so `3,000` against a value of 3 fails
        // rather than matching its prefix.
        assert_eq!(claim("3,000"), Claim::Token("3,000".into()));
        for near in ["2024-02-29, constrained", "359 days", "999 (paren after)", "[1, 2]", r##""x" and more"##] {
            assert!(matches!(claim(near), Claim::Unrecognised(_)), "{near} should be unrecognised, got {:?}", claim(near));
        }
    }

    #[test]
    fn fences_may_be_indented_or_tildes_and_must_close() {
        let b = blocks("- item\n  ```roc exit=2\n  x = 1\n  ```\n\n~~~roc\ny\n~~~\n\n```text\nshown\n```\n").unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].exit, b[0].lines[0].1.as_str()), (2, "x = 1"));
        assert_eq!(b[1].output.as_deref(), Some("shown\n"));
        assert!(blocks("```roc\nx\n").is_err());
    }

    #[test]
    fn a_spread_is_a_use_and_a_field_or_longer_name_is_not() {
        assert!(uses("{ ..jan31, day: 1 }", "jan31"));
        assert!(!uses("d.jan31", "jan31") && !uses("jan31x", "jan31"));
        assert!(!uses(&scan(r##"Greet.hello("who")"##).blanked, "who"));
    }
}
