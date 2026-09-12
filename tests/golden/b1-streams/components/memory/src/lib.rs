//! Test backing: an InputStream over an in-memory buffer, minted through
//! sync-io-core. Also the gauge: `report!` proves stream resources drop-balance.
use trantor_abi as abi;
use abi::RocListWith;
use std::io::Cursor;

/// `Memory.open! : List(U8) => Streams.InputStream`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__memory__open(bytes: RocListWith<u8, false>) -> *mut u64 {
    let owned = bytes.as_slice().to_vec();
    // Owned list arg: release it (B0 rule; no Drop on RocListWith).
    unsafe { bytes.decref(abi::host()) };
    sync_io_core::input_stream(Box::new(Cursor::new(owned))) as *mut u64
}

/// `Memory.report! : U64, U64 => I32` — returns `mem_len` as the exit code iff
/// every stream resource has been dropped (live == 0), else -1.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__memory__report(mem_len: u64, file_len: u64) -> i32 {
    let live = abi::resource::live();
    eprintln!("[memory] report: mem_len={mem_len} file_len={file_len} live={live}");
    if live == 0 { mem_len as i32 } else { -1 }
}
