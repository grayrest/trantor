//! Component `capstdfs`: primitive fs (std::fs stands in for cap-std). Reached
//! host-internally by `audit`, so its symbol is a plain no_mangle Rust fn, not
//! a Roc hosted symbol.
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::{FsFileReadResult, FsFileReadResultPayload, FsFileReadResultTag, FsIOErr, FsIOErrPayload, FsIOErrTag, RocStr};

fn ok(s: &str) -> FsFileReadResult {
    FsFileReadResult {
        payload: FsFileReadResultPayload { ok: ManuallyDrop::new(RocStr::from_str(s, abi::host())) },
        tag: FsFileReadResultTag::Ok,
    }
}
fn err(e: FsIOErr) -> FsFileReadResult {
    FsFileReadResult { payload: FsFileReadResultPayload { err: ManuallyDrop::new(e) }, tag: FsFileReadResultTag::Err }
}
fn not_found() -> FsIOErr {
    FsIOErr { payload: FsIOErrPayload { not_found: [] }, tag: FsIOErrTag::NotFound }
}
fn other(m: &str) -> FsIOErr {
    FsIOErr { payload: FsIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(m, abi::host())) }, tag: FsIOErrTag::Other }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__capstdfs__file_read(path: RocStr) -> FsFileReadResult {
    match std::fs::read_to_string(path.as_str()) {
        Ok(text) => ok(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => err(not_found()),
        Err(e) => err(other(&e.to_string())),
    }
}
