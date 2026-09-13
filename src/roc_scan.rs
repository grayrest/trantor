//! A line of Roc as code: string and char contents blanked, the comment cut
//! off, so its brackets can be counted. Used to find module-level `expect`s
//! (package_modules.rs).

/// The code of `line`: the text before its comment (a `#` outside strings,
/// char literals and interpolations), with string and char contents replaced
/// by spaces. A multi-line string's `${...}` stays code.
pub fn code(line: &str) -> String {
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
        let c = chars[k].1;
        match state {
            S::Code => match c {
                '#' => return blanked,
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
    blanked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_in_a_string_char_or_interpolation_is_not_a_comment() {
        // In each, the comment is the `#` after the three spaces.
        for line in [
            r##"x.format("%A #1")   # "Sunday #1""##,
            r##""${f("#1")}!"   # "x""##,
            r"s.contains('#')   # true",
            r##"f("a").ends_with("\\")   # "x""##,
        ] {
            assert_eq!(code(line).len(), line.rfind("   #").unwrap() + 3, "{line}");
        }
    }

    #[test]
    fn a_multi_line_string_hides_its_text_but_not_its_interpolation() {
        assert!(!code("x = \\\\usage (see below").contains('('));
        assert!(code("msg = \\\\home is ${Env.var_str!(\"HOME\")}").contains("Env.var_str!"));
        assert!(!code("\t\\\\say \"Path.x!\" # not a comment").contains("Path"));
        assert_eq!(code("\t\\\\say # not a comment").len(), "\t\\\\say # not a comment".len());
    }
}
