//! Reading README.md's Roc examples closely enough to check them (T3b): fenced
//! blocks, statements that span lines, where a comment starts, and what a
//! comment claims.

/// One fenced ```roc block.
pub struct Block {
    /// README line of the first body line.
    pub first_line: usize,
    pub lines: Vec<(usize, String)>,
    /// `exit=N` from the fence's info string; a whole app must exit with it.
    pub exit: Option<i32>,
    /// The ```text block that follows it with no other block between — prose
    /// such as "It prints:" is fine — and the README line it starts on: a
    /// whole app's stated stdout.
    pub output: Option<(usize, String)>,
    /// 0-based index of its closing fence.
    close: usize,
}

impl Block {
    #[cfg(test)]
    pub fn for_test(lines: &[&str]) -> Block {
        Block { first_line: 1, lines: lines.iter().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect(), exit: None, output: None, close: 0 }
    }
}

/// Every ```roc (or ~~~roc) block, indented or not. An unterminated fence is
/// an error rather than a block silently dropped.
pub fn blocks(readme: &str) -> Result<Vec<Block>, String> {
    let lines: Vec<&str> = readme.lines().collect();
    let mut out: Vec<Block> = vec![];
    let mut last_close = None;
    let mut i = 0;
    while i < lines.len() {
        let Some((indent, fence, info)) = fence_open(lines[i]) else { i += 1; continue };
        let start = i;
        let mut body = vec![];
        i += 1;
        while i < lines.len() && !is_fence_close(lines[i], &fence) {
            let l = lines[i];
            let stripped = match l.get(..indent) {
                Some(lead) if lead.trim().is_empty() => &l[indent..],
                _ => l.trim_start(),
            };
            body.push((i + 1, stripped.to_string()));
            i += 1;
        }
        if i >= lines.len() {
            return Err(format!("README.md line {}: the ``` block opened here is never closed", start + 1));
        }
        i += 1;
        let lang = info.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        let follows_roc = out.last().is_some_and(|b| Some(b.close) == last_close && b.output.is_none());
        match lang.as_str() {
            "roc" => {
                let exit = info.split_whitespace().find_map(|w| w.strip_prefix("exit=")).map(|n| {
                    n.parse().map_err(|_| format!("README.md line {}: `exit={n}` is not an exit status", start + 1))
                }).transpose()?;
                out.push(Block { first_line: start + 2, lines: body, exit, output: None, close: i - 1 });
            }
            "text" | "output" if follows_roc => {
                let text: String = body.iter().map(|(_, l)| format!("{l}\n")).collect();
                if let Some(b) = out.last_mut() {
                    b.output = Some((start + 1, text));
                }
            }
            _ => {}
        }
        last_close = Some(i - 1);
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
    // A `\\` line is a multi-line string's content, all of it.
    if line.trim_start().starts_with("\\\\") {
        return Scan { comment_at: None, depth: 0, blanked: " ".repeat(line.len()) };
    }
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
    /// The comment on its last line, then any comment-only lines directly
    /// below it: what it claims.
    pub comments: Vec<(usize, String)>,
    /// Comments on its other lines, which are about a part of it and cannot
    /// state its value.
    pub inner: Vec<(usize, String)>,
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
        // A line that can only continue the statement before it: a method
        // chain, a `?`, an operator, or a multi-line string's next line.
        let continues = open.is_none()
            && last_was_stmt
            && [".", "?", "&&", "||", "|>", "->", "=>", "\\\\"].iter().any(|p| code.trim_start().starts_with(p))
            && !code.trim_start().starts_with("..");
        if continues {
            open = stmts.pop();
            depth = 0;
        }
        if let Some(st) = open.as_mut() {
            st.inner.append(&mut st.comments);
            st.code.push('\n');
            st.code.push_str(code);
            st.blanked.push(' ');
            st.blanked.push_str(&blanked);
            depth += sc.depth;
            if let Some(c) = comment {
                if depth <= 0 { st.comments.push((*n, c)) } else { st.inner.push((*n, c)) }
            }
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
        depth = sc.depth;
        let (comments, inner) = match comment {
            Some(c) if depth > 0 => (vec![], vec![(*n, c)]),
            Some(c) => (vec![(*n, c)], vec![]),
            None => (vec![], vec![]),
        };
        let st = Stmt { line: *n, code: code.to_string(), blanked, comments, inner };
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
    fn fences_may_be_indented_or_tildes_and_must_close() {
        let b = blocks("- item\n  ```roc exit=2\n  x = 1\n  ```\n\n~~~roc\ny\n~~~\n\n```text\nshown\n```\n").unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].exit, b[0].lines[0].1.as_str()), (Some(2), "x = 1"));
        assert_eq!(b[1].output, Some((10, "shown\n".to_string())));
        assert!(blocks("```roc\nx\n").is_err());
    }

    #[test]
    fn text_after_prose_is_output_but_not_after_another_block() {
        let b = blocks("```roc\napp [main!] {}\n```\nIt prints:\n```text\nhi\n```\n").unwrap();
        assert_eq!(b[0].output, Some((5, "hi\n".to_string())));
        let b = blocks("```roc\napp [main!] {}\n```\n```sh\nls\n```\n```text\nhi\n```\n").unwrap();
        assert!(b[0].output.is_none());
    }

    #[test]
    fn an_indented_fence_with_a_multibyte_continuation_does_not_panic() {
        let b = blocks("- item\n  ```roc\n  x = 1\n…\n  ```\n").unwrap();
        assert_eq!(b[0].lines[1].1, "…");
    }

    #[test]
    fn a_chain_a_multi_line_string_and_inner_comments_stay_one_statement() {
        let lines: Vec<(usize, String)> = ["x = \"a\"", "\t.concat(\"!\")   # \"a!\"", "s =", "\t\\\\ # 3 is text", "y = Str.concat(", "\t\"a\",   # \"a\"", ")"]
            .iter().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect();
        let (stmts, _) = statements(&lines);
        assert_eq!(stmts.len(), 3, "{:?}", stmts.iter().map(|s| s.code.clone()).collect::<Vec<_>>());
        assert_eq!(stmts[0].comments, vec![(2, "\"a!\"".to_string())]);
        assert!(stmts[1].comments.is_empty() && stmts[1].inner.is_empty(), "a string's # is not a comment");
        assert_eq!((stmts[2].comments.len(), stmts[2].inner.len()), (0, 1));
    }


}
