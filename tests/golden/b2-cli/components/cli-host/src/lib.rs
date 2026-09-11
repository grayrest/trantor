//! roc:cli package host (P9: one crate per package): stdout/stderr/stdin,
//! environment, terminal. Streams are minted through sync-io-core. Blocking
//! (P12). Owned-argument rule (B0): the only refcounted arg is `var`'s name,
//! which is decref'd after use.
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::*;
use std::io::{BufRead, Read};
use std::sync::OnceLock;

// ---- streams: roc:cli/stdout, roc:cli/stderr, roc:cli/stdin ----

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__get_stdout() -> *mut u64 {
    sync_io_core::output_stream(Box::new(std::io::stdout())) as *mut u64
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__get_stderr() -> *mut u64 {
    sync_io_core::output_stream(Box::new(std::io::stderr())) as *mut u64
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__get_stdin() -> *mut u64 {
    sync_io_core::input_stream(Box::new(std::io::stdin())) as *mut u64
}

fn stdin_ioerr(e: &std::io::Error) -> CliInIOErr {
    let tag = match e.kind() {
        std::io::ErrorKind::BrokenPipe => CliInIOErrTag::BrokenPipe,
        std::io::ErrorKind::Interrupted => CliInIOErrTag::Interrupted,
        _ => CliInIOErrTag::Other,
    };
    if let CliInIOErrTag::Other = tag {
        CliInIOErr { payload: CliInIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
    } else {
        CliInIOErr { payload: unsafe { core::mem::zeroed() }, tag }
    }
}

/// `CliIn.read_line! : {} => Try(Str, [EndOfFile, StdinErr(IOErr)])` —
/// std's global Stdin is already buffered, so consecutive calls lose nothing.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__read_line() -> CliInReadLineResult {
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) => CliInReadLineResult {
            payload: CliInReadLineResultPayload { err: ManuallyDrop::new(EndOfFileOrStdinErr {
                payload: EndOfFileOrStdinErrPayload { end_of_file: [] }, tag: EndOfFileOrStdinErrTag::EndOfFile }) },
            tag: CliInReadLineResultTag::Err,
        },
        Ok(_) => {
            let trimmed = line.strip_suffix('\n').map(|s| s.strip_suffix('\r').unwrap_or(s)).unwrap_or(&line);
            CliInReadLineResult { payload: CliInReadLineResultPayload { ok: ManuallyDrop::new(RocStr::from_str(trimmed, abi::host())) }, tag: CliInReadLineResultTag::Ok }
        }
        Err(e) => CliInReadLineResult {
            payload: CliInReadLineResultPayload { err: ManuallyDrop::new(EndOfFileOrStdinErr {
                payload: EndOfFileOrStdinErrPayload { stdin_err: ManuallyDrop::new(stdin_ioerr(&e)) }, tag: EndOfFileOrStdinErrTag::StdinErr }) },
            tag: CliInReadLineResultTag::Err,
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__read_to_end() -> CliInReadToEndResult {
    let mut buf = Vec::new();
    match std::io::stdin().lock().read_to_end(&mut buf) {
        Ok(_) => CliInReadToEndResult {
            payload: CliInReadToEndResultPayload { ok: ManuallyDrop::new(unsafe { RocListWith::<u8, false>::from_slice(&buf, abi::host()) }) },
            tag: CliInReadToEndResultTag::Ok,
        },
        Err(e) => {
            let tag = match e.kind() { std::io::ErrorKind::Interrupted => IOErrTag::Interrupted, _ => IOErrTag::Other };
            let err = if let IOErrTag::Other = tag {
                IOErr { payload: IOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
            } else { IOErr { payload: unsafe { core::mem::zeroed() }, tag } };
            CliInReadToEndResult { payload: CliInReadToEndResultPayload { err: ManuallyDrop::new(err) }, tag: CliInReadToEndResultTag::Err }
        }
    }
}

// ---- roc:cli/environment ----

fn args() -> &'static Vec<String> {
    static A: OnceLock<Vec<String>> = OnceLock::new();
    A.get_or_init(|| std::env::args().skip(1).collect())
}
fn envs() -> &'static Vec<(String, String)> {
    static E: OnceLock<Vec<(String, String)>> = OnceLock::new();
    E.get_or_init(|| std::env::vars().collect())
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__arg_count() -> u64 { args().len() as u64 }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__arg_at(i: u64) -> RocStr {
    RocStr::from_str(args().get(i as usize).map(String::as_str).unwrap_or(""), abi::host())
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__env_count() -> u64 { envs().len() as u64 }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__env_at(i: u64) -> AnonStruct82a96c5d55d63488 {
    let (n, v) = envs().get(i as usize).map(|(n, v)| (n.as_str(), v.as_str())).unwrap_or(("", ""));
    AnonStruct82a96c5d55d63488 { name: RocStr::from_str(n, abi::host()), value: RocStr::from_str(v, abi::host()) }
}

/// `CliEnv.var! : Str => Try(Str, [VarNotFound(Str), EnvErr(IOErr)])`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__var(name: RocStr) -> CliEnvVarResult {
    let key = name.as_str().to_string();
    unsafe { name.decref(abi::host()) }; // owned arg (B0 rule)
    match std::env::var(&key) {
        Ok(v) => CliEnvVarResult { payload: CliEnvVarResultPayload { ok: ManuallyDrop::new(RocStr::from_str(&v, abi::host())) }, tag: CliEnvVarResultTag::Ok },
        Err(_) => CliEnvVarResult {
            payload: CliEnvVarResultPayload { err: ManuallyDrop::new(EnvErrOrVarNotFound {
                payload: EnvErrOrVarNotFoundPayload { var_not_found: ManuallyDrop::new(RocStr::from_str(&key, abi::host())) },
                tag: EnvErrOrVarNotFoundTag::VarNotFound }) },
            tag: CliEnvVarResultTag::Err,
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__platform() -> AnonStructBca0d23b5d625934 {
    let arch = match std::env::consts::ARCH {
        "aarch64" => AARCH64OrARMOrOTHEROrX64OrX86 { payload: AARCH64OrARMOrOTHEROrX64OrX86Payload { aarch64: [] }, tag: AARCH64OrARMOrOTHEROrX64OrX86Tag::AARCH64 },
        "x86_64" => AARCH64OrARMOrOTHEROrX64OrX86 { payload: AARCH64OrARMOrOTHEROrX64OrX86Payload { x64: [] }, tag: AARCH64OrARMOrOTHEROrX64OrX86Tag::X64 },
        "x86" => AARCH64OrARMOrOTHEROrX64OrX86 { payload: AARCH64OrARMOrOTHEROrX64OrX86Payload { x86: [] }, tag: AARCH64OrARMOrOTHEROrX64OrX86Tag::X86 },
        "arm" => AARCH64OrARMOrOTHEROrX64OrX86 { payload: AARCH64OrARMOrOTHEROrX64OrX86Payload { arm: [] }, tag: AARCH64OrARMOrOTHEROrX64OrX86Tag::ARM },
        other => AARCH64OrARMOrOTHEROrX64OrX86 { payload: AARCH64OrARMOrOTHEROrX64OrX86Payload { other: ManuallyDrop::new(RocStr::from_str(other, abi::host())) }, tag: AARCH64OrARMOrOTHEROrX64OrX86Tag::OTHER },
    };
    let os = match std::env::consts::OS {
        "macos" => LINUXOrMACOSOrOTHEROrWINDOWS { payload: LINUXOrMACOSOrOTHEROrWINDOWSPayload { macos: [] }, tag: LINUXOrMACOSOrOTHEROrWINDOWSTag::MACOS },
        "linux" => LINUXOrMACOSOrOTHEROrWINDOWS { payload: LINUXOrMACOSOrOTHEROrWINDOWSPayload { linux: [] }, tag: LINUXOrMACOSOrOTHEROrWINDOWSTag::LINUX },
        "windows" => LINUXOrMACOSOrOTHEROrWINDOWS { payload: LINUXOrMACOSOrOTHEROrWINDOWSPayload { windows: [] }, tag: LINUXOrMACOSOrOTHEROrWINDOWSTag::WINDOWS },
        other => LINUXOrMACOSOrOTHEROrWINDOWS { payload: LINUXOrMACOSOrOTHEROrWINDOWSPayload { other: ManuallyDrop::new(RocStr::from_str(other, abi::host())) }, tag: LINUXOrMACOSOrOTHEROrWINDOWSTag::OTHER },
    };
    AnonStructBca0d23b5d625934 { arch, os }
}

// ---- roc:cli/terminal (raw mode; not exercised by the B2 app) ----
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__enable_raw_mode() {}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__cli_host__disable_raw_mode() {}
