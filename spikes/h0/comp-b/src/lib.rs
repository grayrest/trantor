//! H0 component B. This archive (libb.a) defines ONLY the hosted symbol
//! `trantor__b__pong` — no runtime symbols, no glue, no main. It proves a
//! second, independently built archive can contribute a hosted symbol that
//! the composed platform's `hosted {}` union names.

/// `Host.pong! : {} => U64`.
#[unsafe(no_mangle)]
pub extern "C" fn trantor__b__pong() -> i64 {
    2
}
