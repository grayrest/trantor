//! roc:subprocess host (seahaven's design, P3): spawn via std::process::Command.
//! The crossing record carries seahaven's NativeOsStr union; Utf8 is what Roc
//! mints here, UnixBytes is honored raw. Owned-argument rule (B0): the whole
//! Args struct is owned and released via its own decref (recurses into
//! args/envs element strings), not field-by-field.
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::*;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

type Native = UnixBytesOrUtf8OrWindowsU16s;

// The userland cwd lives in the `cell` component (FsOps.set_cwd! writes it);
// read it to run subprocesses in that directory (Option A cwd model).
unsafe extern "C-unwind" {
    fn hematite__cell__get() -> RocStr;
}

fn to_os(n: &Native) -> OsString {
    unsafe {
        match n.tag {
            UnixBytesOrUtf8OrWindowsU16sTag::Utf8 => OsString::from((*n.payload.utf8).as_str()),
            UnixBytesOrUtf8OrWindowsU16sTag::UnixBytes => OsStr::from_bytes((*n.payload.unix_bytes).as_slice()).to_os_string(),
            UnixBytesOrUtf8OrWindowsU16sTag::WindowsU16s => OsString::from(String::from_utf16_lossy((*n.payload.windows_u16s).as_slice())),
        }
    }
}

/// Build the Command and release the owned Args (B0 rule).
fn command(a: SubprocessHostExecOutputArgs) -> Command {
    let mut c = Command::new(to_os(&a.program));
    c.args(a.args.as_slice().iter().map(to_os));
    if a.clear_envs { c.env_clear(); }
    let envs: Vec<OsString> = a.envs.as_slice().iter().map(to_os).collect();
    for kv in envs.chunks(2) { if let [k, v] = kv { c.env(k, v); } }
    unsafe { a.decref(abi::host()); } // whole-struct decref recurses into args/envs elements (B0)
    // Honor the userland cwd so a child runs where file ops resolve (basic-cli's
    // observable single-cwd behavior), without mutating this process's real cwd.
    // Empty cell = no set_cwd! yet = inherit the process cwd.
    let cwd = unsafe { hematite__cell__get() };
    if !cwd.is_empty() { c.current_dir(cwd.as_str()); }
    unsafe { cwd.decref(abi::host()); }
    c
}
// The four Args structs are layout-identical; view them as one.
fn as_output_args<T>(a: T) -> SubprocessHostExecOutputArgs { unsafe { core::mem::transmute_copy(&ManuallyDrop::new(a)) } }

fn ioerr(e: &std::io::Error) -> IOErr {
    let tag = match e.kind() { std::io::ErrorKind::NotFound => IOErrTag::NotFound, std::io::ErrorKind::PermissionDenied => IOErrTag::PermissionDenied, _ => IOErrTag::Other };
    if let IOErrTag::Other = tag { IOErr { payload: IOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag } } else { IOErr { payload: unsafe { core::mem::zeroed() }, tag } }
}
fn sp_ioerr(e: &std::io::Error) -> SubprocessHostIOErr {
    let tag = match e.kind() { std::io::ErrorKind::NotFound => SubprocessHostIOErrTag::NotFound, std::io::ErrorKind::PermissionDenied => SubprocessHostIOErrTag::PermissionDenied, _ => SubprocessHostIOErrTag::Other };
    if let SubprocessHostIOErrTag::Other = tag { SubprocessHostIOErr { payload: SubprocessHostIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag } } else { SubprocessHostIOErr { payload: unsafe { core::mem::zeroed() }, tag } }
}
fn bytes(v: &[u8]) -> RocListWith<u8, false> { unsafe { RocListWith::<u8, false>::from_slice(v, abi::host()) } }

fn exit_result(r: std::io::Result<std::process::ExitStatus>, signal_negative: bool) -> SubprocessHostExecExitCodeResult {
    match r {
        Ok(st) => {
            let code = st.code().unwrap_or_else(|| if signal_negative { -(st.signal().unwrap_or(0)) } else { -1 });
            SubprocessHostExecExitCodeResult { payload: SubprocessHostExecExitCodeResultPayload { ok: ManuallyDrop::new(code) }, tag: SubprocessHostExecExitCodeResultTag::Ok }
        }
        Err(e) => SubprocessHostExecExitCodeResult { payload: SubprocessHostExecExitCodeResultPayload { err: ManuallyDrop::new(sp_ioerr(&e)) }, tag: SubprocessHostExecExitCodeResultTag::Err },
    }
}
fn output_result(mut c: Command, inherit_stdin: bool) -> SubprocessHostExecOutputResult {
    if inherit_stdin { c.stdin(Stdio::inherit()); }
    match c.output() {
        Ok(o) if o.status.success() => SubprocessHostExecOutputResult {
            payload: SubprocessHostExecOutputResultPayload { ok: ManuallyDrop::new(AnonStruct3e7554e024207e25 { stderr_bytes: bytes(&o.stderr), stdout_bytes: bytes(&o.stdout) }) },
            tag: SubprocessHostExecOutputResultTag::Ok,
        },
        Ok(o) => SubprocessHostExecOutputResult {
            payload: SubprocessHostExecOutputResultPayload { err: ManuallyDrop::new(FailedToGetExitCodeOrNonZeroExitCode {
                payload: FailedToGetExitCodeOrNonZeroExitCodePayload { non_zero_exit_code: ManuallyDrop::new(AnonStruct3f89ee1e14924626 { stderr_bytes: bytes(&o.stderr), stdout_bytes: bytes(&o.stdout), exit_code: o.status.code().unwrap_or(-1) }) },
                tag: FailedToGetExitCodeOrNonZeroExitCodeTag::NonZeroExitCode }) },
            tag: SubprocessHostExecOutputResultTag::Err,
        },
        Err(e) => SubprocessHostExecOutputResult {
            payload: SubprocessHostExecOutputResultPayload { err: ManuallyDrop::new(FailedToGetExitCodeOrNonZeroExitCode {
                payload: FailedToGetExitCodeOrNonZeroExitCodePayload { failed_to_get_exit_code: ManuallyDrop::new(ioerr(&e)) },
                tag: FailedToGetExitCodeOrNonZeroExitCodeTag::FailedToGetExitCode }) },
            tag: SubprocessHostExecOutputResultTag::Err,
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__subprocess_host__exec_exit_code(a: SubprocessHostExecExitCodeArgs) -> SubprocessHostExecExitCodeResult {
    exit_result(command(as_output_args(a)).status(), false)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__subprocess_host__exec_status(a: SubprocessHostExecStatusArgs) -> SubprocessHostExecExitCodeResult {
    exit_result(command(as_output_args(a)).status(), true)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__subprocess_host__exec_output(a: SubprocessHostExecOutputArgs) -> SubprocessHostExecOutputResult {
    output_result(command(a), false)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__subprocess_host__exec_output_inherit_stdin(a: SubprocessHostExecOutputInheritStdinArgs) -> SubprocessHostExecOutputResult {
    output_result(command(as_output_args(a)), true)
}
