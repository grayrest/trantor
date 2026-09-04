//! roc:sync-io/streams host: the hosted read!/write! symbols over the stream
//! resources minted by sync-io-core. Blocking (P12). Every hosted arg is OWNED
//! (glue contract, B0): the stream handle is released via `resource::with`,
//! and list args are `.decref`'d — no Drop exists on bare RocListWith.
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::{RocBox, RocListWith, RocStr,
    StreamsReadResult as ReadR, StreamsReadResultPayload as ReadP, StreamsReadResultTag as ReadT,
    StreamsWriteResult as WriteR, StreamsWriteResultPayload as WriteP, StreamsWriteResultTag as WriteT,
    StreamsIOErr, StreamsIOErrPayload, StreamsIOErrTag, IOErr, IOErrPayload, IOErrTag};
use std::io::Read;
use sync_io_core::{Input, Output};

fn read_err(e: &std::io::Error) -> ReadR {
    let tag = match e.kind() {
        std::io::ErrorKind::NotFound => StreamsIOErrTag::NotFound,
        std::io::ErrorKind::PermissionDenied => StreamsIOErrTag::PermissionDenied,
        std::io::ErrorKind::BrokenPipe => StreamsIOErrTag::BrokenPipe,
        std::io::ErrorKind::Interrupted => StreamsIOErrTag::Interrupted,
        _ => StreamsIOErrTag::Other,
    };
    let err = if let StreamsIOErrTag::Other = tag {
        StreamsIOErr { payload: StreamsIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
    } else {
        StreamsIOErr { payload: unsafe { core::mem::zeroed() }, tag }
    };
    ReadR { payload: ReadP { err: ManuallyDrop::new(err) }, tag: ReadT::Err }
}
fn write_err(e: &std::io::Error) -> WriteR {
    let tag = match e.kind() {
        std::io::ErrorKind::BrokenPipe => IOErrTag::BrokenPipe,
        std::io::ErrorKind::Interrupted => IOErrTag::Interrupted,
        _ => IOErrTag::Other,
    };
    let err = if let IOErrTag::Other = tag {
        IOErr { payload: IOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
    } else {
        IOErr { payload: unsafe { core::mem::zeroed() }, tag }
    };
    WriteR { payload: WriteP { err: ManuallyDrop::new(err) }, tag: WriteT::Err }
}

/// `Streams.read! : InputStream, U64 => Try(List(U8), [StreamErr(IOErr)])`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__sync_io__read(s: *mut u64, max: u64) -> ReadR {
    // Owned handle: `with` borrows then releases (B0 rule).
    let outcome: std::io::Result<Vec<u8>> = unsafe {
        abi::resource::with(s as RocBox, |inp: &mut Input| {
            let mut buf = vec![0u8; max as usize];
            let n = inp.0.read(&mut buf)?;
            buf.truncate(n);
            Ok(buf)
        })
    };
    match outcome {
        Ok(bytes) => {
            eprintln!("[sync-io] read {} bytes", bytes.len());
            let list = unsafe { RocListWith::<u8, false>::from_slice(&bytes, abi::host()) };
            ReadR { payload: ReadP { ok: ManuallyDrop::new(list) }, tag: ReadT::Ok }
        }
        Err(e) => read_err(&e),
    }
}

/// `Streams.write! : OutputStream, List(U8) => Try({}, [StreamErr(IOErr)])`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__sync_io__write(s: *mut u64, bytes: RocListWith<u8, false>) -> WriteR {
    let outcome: std::io::Result<()> = unsafe {
        abi::resource::with(s as RocBox, |out: &mut Output| out.0.write_all(bytes.as_slice()))
    };
    // Owned list arg: release it (no Drop on RocListWith).
    unsafe { bytes.decref(abi::host()) };
    match outcome {
        Ok(()) => WriteR { payload: WriteP { ok: [] }, tag: WriteT::Ok },
        Err(e) => write_err(&e),
    }
}
