//! H0 component B. This archive (libb.a) defines ONLY the hosted symbol
//! `hematite__b__pong` — no runtime symbols, no glue, no main. It proves a
//! second, independently built archive can contribute a hosted symbol that
//! the composed platform's `hosted {}` union names.

/// `Host.pong! : {} => U64`.
unsafe extern "C" { fn vendored_answer() -> i32; }

#[unsafe(no_mangle)]
pub extern "C" fn hematite__b__pong() -> i64 {
    (unsafe { vendored_answer() }) as i64
}
