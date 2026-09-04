//! Component `stdio`: host component exporting one hosted symbol.
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::{StdioLineResult, StdioLineResultPayload, StdioLineResultTag, StdioIOErr, StdioIOErrPayload, StdioIOErrTag, RocStr};
use std::io::Write;

fn ok() -> StdioLineResult {
    StdioLineResult { payload: StdioLineResultPayload { ok: [] }, tag: StdioLineResultTag::Ok }
}
fn err_other(m: &str) -> StdioLineResult {
    let e = StdioIOErr { payload: StdioIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(m, abi::host())) }, tag: StdioIOErrTag::Other };
    StdioLineResult { payload: StdioLineResultPayload { err: ManuallyDrop::new(e) }, tag: StdioLineResultTag::Err }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__stdio__stdout_line(line: RocStr) -> StdioLineResult {
    let r = writeln!(std::io::stdout(), "{}", line.as_str());
    unsafe { line.decref(abi::host()); } // owned arg (B0): released after use
    match r {
        Ok(()) => ok(),
        Err(e) => err_other(&e.to_string()),
    }
}
