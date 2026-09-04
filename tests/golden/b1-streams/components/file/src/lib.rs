//! Test backing: an InputStream over a real file (stand-in for B3's
//! descriptor.read-via-stream), minted through sync-io-core.
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::{RocStr, FileIoOpenResult as OpenR, FileIoOpenResultPayload as OpenP, FileIoOpenResultTag as OpenT,
    FileIoIOErr, FileIoIOErrPayload, FileIoIOErrTag};

fn open_err(e: &std::io::Error) -> OpenR {
    let tag = match e.kind() {
        std::io::ErrorKind::NotFound => FileIoIOErrTag::NotFound,
        std::io::ErrorKind::PermissionDenied => FileIoIOErrTag::PermissionDenied,
        _ => FileIoIOErrTag::Other,
    };
    let err = if let FileIoIOErrTag::Other = tag {
        FileIoIOErr { payload: FileIoIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
    } else {
        FileIoIOErr { payload: unsafe { core::mem::zeroed() }, tag }
    };
    OpenR { payload: OpenP { err: ManuallyDrop::new(err) }, tag: OpenT::Err }
}

/// `FileIo.open! : Str => Try(Streams.InputStream, [FileErr(IOErr)])`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__file__open(path: RocStr) -> OpenR {
    let opened = std::fs::File::open(path.as_str());
    // Owned str arg: release it (B0 rule; no Drop on RocStr).
    unsafe { path.decref(abi::host()) };
    match opened {
        Ok(f) => {
            let handle = sync_io_core::input_stream(Box::new(f)) as *mut u64;
            OpenR { payload: OpenP { ok: ManuallyDrop::new(handle) }, tag: OpenT::Ok }
        }
        Err(e) => open_err(&e),
    }
}
