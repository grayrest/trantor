//! Numbers in README claims and inspected values, compared as decimal text.

/// A decimal literal without a `+`, trailing fractional zeros or a bare `.`:
/// `3.0` and `3` are one number. Anything else is returned as is.
pub fn normal_number(t: &str) -> String {
    let t = t.strip_prefix('+').unwrap_or(t);
    let digits = t.strip_prefix('-').unwrap_or(t);
    let valid = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '.') && digits.matches('.').count() <= 1 && digits.starts_with(|c: char| c.is_ascii_digit());
    if !valid || !t.contains('.') {
        return if valid && t == "-0" { "0".into() } else { t.to_string() };
    }
    let trimmed = t.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" { "0".into() } else { trimmed.to_string() }
}

/// Every number outside quotes in an inspected value, normalised.
pub fn normal_numbers(t: &str) -> String {
    let (mut out, mut word, mut quoted, mut escaped) = (String::new(), String::new(), false, false);
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(&normal_number(word));
        word.clear();
    };
    for c in t.chars() {
        if quoted {
            out.push(c);
            match c { _ if escaped => escaped = false, '\\' => escaped = true, '"' => quoted = false, _ => {} }
        } else if c.is_ascii_digit() || c == '.' || (c == '-' && word.is_empty()) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            if c == '"' { quoted = true }
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}
