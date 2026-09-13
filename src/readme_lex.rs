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
    let mut last_heading = None;
    let mut i = 0;
    while i < lines.len() {
        let Some((indent, fence, info)) = fence_open(lines[i]) else {
            // A setext heading is its underline under a paragraph line.
            let underline = !lines[i].starts_with("    ") && { let t = lines[i].trim(); !t.is_empty() && (t.chars().all(|c| c == '=') || t.chars().all(|c| c == '-')) };
            // Only under a paragraph line: `---` under a fence, a list item or a
            // heading is a thematic break.
            let setext = underline && i > 0 && is_paragraph_line(lines[i - 1]) && last_close != Some(i - 1);
            if is_heading(lines[i]) || setext {
                last_heading = Some(i);
            }
            i += 1;
            continue;
        };
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
        // Output follows its app with prose between at most: another block
        // or a new heading means it is about something else.
        let follows_roc = out.last().is_some_and(|b| Some(b.close) == last_close && b.output.is_none() && last_heading.is_none_or(|h| h < b.close));
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

/// An opening fence: its indent, the fence run (three or more backticks or
/// tildes — a longer run lets a block show a shorter one), and the info.
fn fence_open(line: &str) -> Option<(usize, String, String)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = t.chars().take_while(|x| *x == c).count();
    if run < 3 {
        return None;
    }
    let info = &t[run..];
    if c == '`' && info.contains('`') {
        return None;
    }
    Some((indent, t[..run].to_string(), info.trim().to_string()))
}

/// A closing fence: the same character, at least as many, and nothing else.
fn is_fence_close(line: &str, fence: &str) -> bool {
    let t = line.trim();
    let c = fence.chars().next().unwrap_or('`');
    t.len() >= fence.len() && t.chars().all(|x| x == c)
}

fn is_paragraph_line(line: &str) -> bool {
    let t = line.trim_start();
    let list_item = ["- ", "* ", "+ "].iter().any(|p| t.starts_with(p)) || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    !t.is_empty() && !list_item && !is_heading(line) && !t.starts_with("```") && !t.starts_with("~~~") && !t.starts_with('>')
}

/// An ATX heading: up to three spaces, then 1–6 `#`. Indented further it is
/// code, not a heading.
fn is_heading(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 || line.starts_with('\t') {
        return false;
    }
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && t[hashes..].chars().next().is_none_or(char::is_whitespace)
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
    #[derive(PartialEq, Clone, Copy)]
    enum S { Code, Str, Multi, Char }
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    // A `\\` line is a multi-line string's content; its `${...}` is still code.
    let lead = line.len() - line.trim_start().len();
    let (mut state, mut k) = if line[lead..].starts_with("\\\\") { (S::Multi, chars.iter().position(|(i, _)| *i == lead + 2).unwrap_or(chars.len())) } else { (S::Code, 0) };
    let mut depth = 0i32;
    // Each open interpolation: the depth it opened at, and the string it returns to.
    let mut interp: Vec<(i32, S)> = vec![];
    let mut blanked = " ".repeat(line[..chars.get(k).map_or(line.len(), |c| c.0)].chars().count());
    while k < chars.len() {
        let (at, c) = chars[k];
        match state {
            S::Code => match c {
                '#' => return Scan { comment_at: Some(at), depth, blanked },
                // `x = \\text`: a multi-line string starting mid-line runs to
                // the end of it, brackets and all.
                '\\' if chars.get(k + 1).map(|x| x.1) == Some('\\') => {
                    state = S::Multi;
                    blanked.push_str("  ");
                    k += 1;
                }
                '"' => { state = S::Str; blanked.push('"') }
                '\'' => { state = S::Char; blanked.push('\'') }
                '(' | '[' | '{' => { depth += 1; blanked.push(c) }
                ')' | ']' => { depth -= 1; blanked.push(c) }
                '}' => {
                    if interp.last().is_some_and(|(d, _)| *d == depth) {
                        state = interp.pop().map_or(S::Str, |(_, back)| back);
                        blanked.push(' ');
                    } else {
                        depth -= 1;
                        blanked.push(c);
                    }
                }
                _ => blanked.push(c),
            },
            S::Str | S::Multi => match c {
                '\\' if state == S::Str => { blanked.push_str("  "); k += 1 }
                '"' if state == S::Str => { state = S::Code; blanked.push('"') }
                '$' if chars.get(k + 1).map(|x| x.1) == Some('{') => {
                    interp.push((depth, state));
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
        let lead = code.trim_start();
        let previous_expects_more = stmts.last().is_some_and(|s| {
            let end = s.code.trim_end();
            (end.ends_with('=') && !end.ends_with("==")) || end.ends_with("->") || end.ends_with("=>") || end.ends_with('|')
        });
        let continues = open.is_none()
            && last_was_stmt
            && (([".", "?", "&&", "||", "|>", "->", "=>", "\\\\"].iter().any(|p| lead.starts_with(p)) && !lead.starts_with(".."))
                || (previous_expects_more && code.starts_with(char::is_whitespace)));
        if continues {
            open = stmts.pop();
            depth = 0;
            // Comment-only lines it passed over stay as empty lines, so the
            // statement's code keeps one line per README line — build errors
            // are mapped back by that count.
            if let Some(st) = open.as_mut() {
                let last = st.line + st.code.lines().count().saturating_sub(1);
                for _ in last + 1..*n {
                    st.code.push('\n');
                }
            }
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
#[path = "readme_lex_tests.rs"]
mod tests;
