//! Tests for `readme_lex.rs`.
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
fn a_longer_fence_holds_a_shorter_one_and_a_heading_ends_output() {
    let b = blocks("````markdown\n```roc\nx\n```\n````\n\n```roc\ny   # 1\n```\n").unwrap();
    assert_eq!(b.len(), 1, "the roc block inside the markdown example is not one");
    assert_eq!(b[0].lines[0].1, "y   # 1");
    let b = blocks("```roc\napp [main!] {}\n```\n## Installing\n```text\ntrantor add org/greet\n```\n").unwrap();
    assert!(b[0].output.is_none());
}

#[test]
fn an_indented_hash_line_is_not_a_heading_but_a_setext_one_is() {
    let b = blocks("```roc\napp [main!] {}\n```\nRun it:\n\n    # from the package root\n\nIt prints:\n```text\nhi\n```\n").unwrap();
    assert!(b[0].output.is_some(), "an indented # line is code");
    let b = blocks("```roc\napp [main!] {}\n```\nInstalling\n----------\n```text\ntrantor add org/greet\n```\n").unwrap();
    assert!(b[0].output.is_none(), "a setext heading ends the app's output");
    let b = blocks("```roc\napp [main!] {}\n```\n---\n\n```text\nhi\n```\n").unwrap();
    assert!(b[0].output.is_some(), "`---` under a fence is a thematic break, not a heading");
}

#[test]
fn a_multi_line_string_hides_its_text_but_not_its_interpolation() {
    assert_eq!(scan("x = \\\\usage (see below").depth, 0);
    assert!(scan("msg = \\\\home is ${Env.var_str!(\"HOME\")}").blanked.contains("Env.var_str!"));
    assert!(scan("\t\\\\home is ${Env.var!(\"HOME\")}").blanked.contains("Env.var!"));
    assert!(!scan("\t\\\\say \"Path.x!\" # not a comment").blanked.contains("Path"));
    assert_eq!(scan("\t\\\\say # not a comment").comment_at, None);
}

#[test]
fn a_comment_line_inside_a_chain_keeps_its_line() {
    let lines: Vec<(usize, String)> = ["x = Greet.hello(who)", "  # a note about the chain", "  .nope()"]
        .iter().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect();
    let (stmts, _) = statements(&lines);
    assert_eq!(stmts.len(), 1);
    assert_eq!(stmts[0].code.lines().count(), 3, "{:?}", stmts[0].code);
}

#[test]
fn a_value_on_the_next_indented_line_belongs_to_its_binding() {
    let lines: Vec<(usize, String)> = ["greeting =", "\tGreet.hello(who)   # \"hello world\""]
        .iter().enumerate().map(|(i, l)| (i + 1, l.to_string())).collect();
    let (stmts, _) = statements(&lines);
    assert_eq!(stmts.len(), 1);
    assert_eq!(stmts[0].comments.len(), 1);
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
