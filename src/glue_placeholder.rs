//! The second glue run for multi-field single-variant unions (D-H7-44): a copy
//! of the platform's Roc modules in which each such union has a placeholder
//! second variant, so glue writes the payload struct it will not write for the
//! real union. Only this run sees the placeholder.
use std::path::Path;

use crate::glue_unions::Single;

/// The placeholder variant the second glue run sees.
pub const PLACEHOLDER: &str = "TrantorGlueOnly";

/// Glue's output for a copy of the platform's Roc modules in which each
/// multi-field single-variant union has a placeholder second variant: there
/// glue writes the payload struct it will not write for the real union.
pub fn placeholder_glue(dir: &Path, gen: &Path, singles: &[Single]) -> Result<String, String> {
    let copy = gen.join("glue-placeholder");
    let _ = std::fs::remove_dir_all(&copy);
    std::fs::create_dir_all(copy.join("platform")).map_err(|e| format!("create {}: {e}", copy.display()))?;
    std::fs::create_dir_all(copy.join("out")).map_err(|e| format!("create {}: {e}", copy.display()))?;
    for e in std::fs::read_dir(gen.join("platform")).map_err(|e| format!("read platform: {e}"))?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "roc") {
            let mut text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(s) = singles.iter().find(|s| s.fields > 1 && s.module == stem) {
                text = with_placeholder(&text, &s.module).ok_or_else(|| format!("{}: could not add the placeholder variant", p.display()))?;
            }
            std::fs::write(copy.join("platform").join(e.file_name()), text).map_err(|e| format!("write the placeholder platform: {e}"))?;
        }
    }
    let out = crate::build::abs(&copy.join("out"))?;
    let main = crate::build::abs(&copy.join("platform/main.roc"))?;
    crate::build::roc_capped(&["glue", &crate::build::glue_src(), &out, &main], dir, "glue (placeholder copy)")?;
    std::fs::read_to_string(copy.join("out/roc_platform_abi.rs")).map_err(|e| format!("read placeholder glue output: {e}"))
}


/// The module text with a placeholder second variant, for the second glue run:
/// a comma after the last variant's code (not after a comment on its line),
/// and the placeholder on a line of its own before the closing `]`.
pub fn with_placeholder(text: &str, name: &str) -> Option<String> {
    let start = text.find(&format!("{name} :="))?;
    let open = text[start..].find('[')? + start;
    let mut depth = 0i32;
    let mut in_comment = false;
    let mut last_code = open;
    for (i, c) in text[open..].char_indices() {
        let at = open + i;
        match c {
            _ if in_comment => in_comment = c != '\n',
            '#' => in_comment = true,
            '(' | '[' | '{' => { depth += 1; last_code = at; }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    let comma = if text[last_code..].starts_with(',') || last_code == open { "" } else { "," };
                    return Some(format!("{}{comma}{}\n{PLACEHOLDER},\n{}", &text[..=last_code], &text[last_code + 1..at], &text[at..]));
                }
                last_code = at;
            }
            c if !c.is_whitespace() => last_code = at,
            _ => {}
        }
    }
    None
}
