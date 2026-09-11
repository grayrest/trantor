//! `std-stdio`: implements the 6 stdio primitives against real std::io. One of
//! two interchangeable implementations of the SAME interface seahaven's real
//! Stdout/Stderr derived layer wraps (H5 substitution proof). Every boundary
//! extern "C-unwind" (H0d).
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::{RocStr, RocListWith,
    StdioStdoutLineResult as OutR, StdioStdoutLineResultPayload as OutP, StdioStdoutLineResultTag as OutT,
    StdioStderrLineResult as ErrR, StdioStderrLineResultPayload as ErrP, StdioStderrLineResultTag as ErrT,
    StdioIOErr, StdioIOErrPayload, StdioIOErrTag,
    IOErr, IOErrPayload, IOErrTag};
use std::io::Write;

fn io_err(e: &std::io::Error) -> StdioIOErr {
    use std::io::ErrorKind::*;
    let tag = match e.kind() {
        NotFound => StdioIOErrTag::NotFound,
        PermissionDenied => StdioIOErrTag::PermissionDenied,
        BrokenPipe => StdioIOErrTag::BrokenPipe,
        Interrupted => StdioIOErrTag::Interrupted,
        _ => StdioIOErrTag::Other,
    };
    if let StdioIOErrTag::Other = tag {
        StdioIOErr { payload: StdioIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
    } else {
        // non-Other variants carry no payload; zero the union.
        StdioIOErr { payload: unsafe { core::mem::zeroed() }, tag }
    }
}
fn out_ok() -> OutR { OutR { payload: OutP { ok: [] }, tag: OutT::Ok } }
fn out_err(e: &std::io::Error) -> OutR { OutR { payload: OutP { err: ManuallyDrop::new(io_err(e)) }, tag: OutT::Err } }
fn io_err_e(e: &std::io::Error) -> IOErr {
    use std::io::ErrorKind::*;
    let tag = match e.kind() {
        NotFound => IOErrTag::NotFound,
        PermissionDenied => IOErrTag::PermissionDenied,
        BrokenPipe => IOErrTag::BrokenPipe,
        Interrupted => IOErrTag::Interrupted,
        _ => IOErrTag::Other,
    };
    if let IOErrTag::Other = tag {
        IOErr { payload: IOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
    } else {
        IOErr { payload: unsafe { core::mem::zeroed() }, tag }
    }
}
fn err_ok() -> ErrR { ErrR { payload: ErrP { ok: [] }, tag: ErrT::Ok } }
fn err_err(e: &std::io::Error) -> ErrR { ErrR { payload: ErrP { err: ManuallyDrop::new(io_err_e(e)) }, tag: ErrT::Err } }

// Owned-argument rule (B0): each fn releases its RocStr / byte-list arg after
// writing (plain decref = full release; no refcounted elements here).
#[unsafe(no_mangle)] pub extern "C-unwind" fn trantor__std_stdio__stdout_line(s: RocStr) -> OutR {
    let r = writeln!(std::io::stdout(), "{}", s.as_str());
    unsafe { s.decref(abi::host()); }
    match r { Ok(()) => out_ok(), Err(e) => out_err(&e) }
}
#[unsafe(no_mangle)] pub extern "C-unwind" fn trantor__std_stdio__stdout_write(s: RocStr) -> OutR {
    let r = write!(std::io::stdout(), "{}", s.as_str());
    unsafe { s.decref(abi::host()); }
    match r { Ok(()) => out_ok(), Err(e) => out_err(&e) }
}
#[unsafe(no_mangle)] pub extern "C-unwind" fn trantor__std_stdio__stdout_write_bytes(b: RocListWith<u8, false>) -> OutR {
    let r = std::io::stdout().write_all(b.as_slice());
    unsafe { b.decref(abi::host()); }
    match r { Ok(()) => out_ok(), Err(e) => out_err(&e) }
}
#[unsafe(no_mangle)] pub extern "C-unwind" fn trantor__std_stdio__stderr_line(s: RocStr) -> ErrR {
    let r = writeln!(std::io::stderr(), "{}", s.as_str());
    unsafe { s.decref(abi::host()); }
    match r { Ok(()) => err_ok(), Err(e) => err_err(&e) }
}
#[unsafe(no_mangle)] pub extern "C-unwind" fn trantor__std_stdio__stderr_write(s: RocStr) -> ErrR {
    let r = write!(std::io::stderr(), "{}", s.as_str());
    unsafe { s.decref(abi::host()); }
    match r { Ok(()) => err_ok(), Err(e) => err_err(&e) }
}
#[unsafe(no_mangle)] pub extern "C-unwind" fn trantor__std_stdio__stderr_write_bytes(b: RocListWith<u8, false>) -> ErrR {
    let r = std::io::stderr().write_all(b.as_slice());
    unsafe { b.decref(abi::host()); }
    match r { Ok(()) => err_ok(), Err(e) => err_err(&e) }
}
