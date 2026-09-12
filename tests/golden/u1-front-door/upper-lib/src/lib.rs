//! The "library" half of the walkthrough: the work done in Rust rather than by
//! a subprocess.
pub fn shout(s: &str) -> String {
    s.to_uppercase()
}
