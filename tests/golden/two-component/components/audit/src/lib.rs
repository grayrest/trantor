//! Component `audit`: interposer over `fs` (imports fs, exports fs). Its export
//! is the app-facing head hosted symbol; it logs then delegates to capstdfs.
use hematite_abi::{FsFileReadResult, RocStr};

unsafe extern "C-unwind" {
    fn hematite__capstdfs__file_read(path: RocStr) -> FsFileReadResult;
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__audit__file_read(path: RocStr) -> FsFileReadResult {
    eprintln!("[audit] file_read {}", path.as_str());
    // Transfer ownership of `path` to capstdfs by moving it into the call; the
    // callee releases it, so this interposer must NOT decref here (B0).
    unsafe { hematite__capstdfs__file_read(path) }
}
