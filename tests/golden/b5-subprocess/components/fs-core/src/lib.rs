//! fs-core (P9 shared substrate, rlib, NO no_mangle): the roc:filesystem op
//! bodies, the root policy, and the `exports!` macro that a thin staticlib
//! (fs-unconfined / fs-confined) invokes to emit its 16 hosted symbols under
//! its own prefix. Capability model (P4): every op is relative to a Descriptor;
//! the ONLY policy difference between the two components is `Root`.
//!
//! Owned-argument rule (B0): every hosted fn releases its Descriptor via
//! `resource::with` and `.decref`s every list arg.
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::*;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub use paste;

/// The one policy knob: where the preopen points, and whether escapes are refused.
pub struct Root {
    pub base: PathBuf,
    pub confined: bool,
}

/// The Descriptor resource payload.
pub enum Desc {
    Dir(PathBuf),
    File(std::fs::File),
}

fn dir_of(d: &Desc) -> PathBuf {
    match d {
        Desc::Dir(p) => p.clone(),
        Desc::File(_) => PathBuf::from("."),
    }
}

/// Canonicalize leniently: the deepest existing ancestor is canonicalized and
/// the non-existent tail is re-appended (so a to-be-created path still checks).
fn canon_lenient(p: &Path) -> std::io::Result<PathBuf> {
    let mut existing = p.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(n), Some(par)) => {
                tail.push(n.to_os_string());
                existing = par.to_path_buf();
            }
            _ => break,
        }
    }
    let mut out = existing.canonicalize()?;
    for seg in tail.iter().rev() {
        out.push(seg);
    }
    Ok(out)
}

/// Resolve `rel` (raw bytes) against a directory descriptor under the root
/// policy. Confined roots refuse any resolution outside `root.base`.
pub fn resolve(root: &Root, dir: &Path, rel: &[u8]) -> std::io::Result<PathBuf> {
    let p = Path::new(std::ffi::OsStr::from_bytes(rel));
    let cand = if p.is_absolute() { p.to_path_buf() } else { dir.join(p) };
    if root.confined {
        let rootc = root.base.canonicalize()?;
        let c = canon_lenient(&cand)?;
        if !c.starts_with(&rootc) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "outside preopen"));
        }
    }
    Ok(cand)
}

// ---- error twins (glue emits one IOErr per reach path; both are the same 10 variants) ----
macro_rules! ioerr_ctor {
    ($name:ident, $ty:ident, $pl:ident, $tag:ident) => {
        pub fn $name(e: &std::io::Error) -> $ty {
            use std::io::ErrorKind as K;
            let tag = match e.kind() {
                K::NotFound => $tag::NotFound,
                K::PermissionDenied => $tag::PermissionDenied,
                K::AlreadyExists => $tag::AlreadyExists,
                K::BrokenPipe => $tag::BrokenPipe,
                K::Interrupted => $tag::Interrupted,
                K::Unsupported => $tag::Unsupported,
                _ => $tag::Other,
            };
            if let $tag::Other = tag {
                $ty { payload: $pl { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag }
            } else {
                $ty { payload: unsafe { core::mem::zeroed() }, tag }
            }
        }
    };
}
ioerr_ctor!(ioerr, IOErr, IOErrPayload, IOErrTag);
ioerr_ctor!(fs_ioerr, FsIOErr, FsIOErrPayload, FsIOErrTag);

fn bytes(l: &RocListWith<u8, false>) -> Vec<u8> {
    l.as_slice().to_vec()
}

// ---- op bodies ----
pub mod ops {
    use super::*;

    pub fn preopen_count(_r: &Root) -> u64 { 1 }
    pub fn preopen_at(r: &Root, _i: u64) -> *mut u64 {
        abi::resource::new(Desc::Dir(r.base.clone())) as *mut u64
    }

    pub fn open_at(r: &Root, d: *mut u64, path: RocListWith<u8, false>, flags: u8) -> FsOpenAtResult {
        let rel = bytes(&path); unsafe { path.decref(abi::host()) };
        let dir = unsafe { abi::resource::with(d as RocBox, |x: &mut Desc| dir_of(x)) };
        let res = resolve(r, &dir, &rel).and_then(|p| if flags == 1 { std::fs::File::create(p) } else { std::fs::File::open(p) });
        match res {
            Ok(f) => FsOpenAtResult { payload: FsOpenAtResultPayload { ok: ManuallyDrop::new(abi::resource::new(Desc::File(f)) as *mut u64) }, tag: FsOpenAtResultTag::Ok },
            Err(e) => FsOpenAtResult { payload: FsOpenAtResultPayload { err: ManuallyDrop::new(fs_ioerr(&e)) }, tag: FsOpenAtResultTag::Err },
        }
    }

    pub fn read_via_stream(_r: &Root, d: *mut u64) -> *mut u64 {
        let reader: Box<dyn std::io::Read> = unsafe {
            abi::resource::with(d as RocBox, |x: &mut Desc| match x {
                Desc::File(f) => f.try_clone().map(|f| Box::new(f) as Box<dyn std::io::Read>).unwrap_or_else(|_| Box::new(std::io::empty())),
                Desc::Dir(_) => Box::new(std::io::empty()),
            })
        };
        sync_io_core::input_stream(reader) as *mut u64
    }

    fn with_path<T>(r: &Root, d: *mut u64, path: RocListWith<u8, false>, f: impl FnOnce(PathBuf) -> std::io::Result<T>) -> std::io::Result<T> {
        let rel = bytes(&path); unsafe { path.decref(abi::host()) };
        let dir = unsafe { abi::resource::with(d as RocBox, |x: &mut Desc| dir_of(x)) };
        resolve(r, &dir, &rel).and_then(f)
    }

    pub fn read_file_at(r: &Root, d: *mut u64, path: RocListWith<u8, false>) -> FsReadFileAtResult {
        match with_path(r, d, path, std::fs::read) {
            Ok(v) => FsReadFileAtResult { payload: FsReadFileAtResultPayload { ok: ManuallyDrop::new(unsafe { RocListWith::<u8, false>::from_slice(&v, abi::host()) }) }, tag: FsReadFileAtResultTag::Ok },
            Err(e) => FsReadFileAtResult { payload: FsReadFileAtResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: FsReadFileAtResultTag::Err },
        }
    }

    fn unit(res: std::io::Result<()>) -> FsWriteFileAtResult {
        match res {
            Ok(()) => FsWriteFileAtResult { payload: FsWriteFileAtResultPayload { ok: [] }, tag: FsWriteFileAtResultTag::Ok },
            Err(e) => FsWriteFileAtResult { payload: FsWriteFileAtResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: FsWriteFileAtResultTag::Err },
        }
    }
    fn dir_unit(res: std::io::Result<()>) -> FsCreateDirAtResult {
        match res {
            Ok(()) => FsCreateDirAtResult { payload: FsCreateDirAtResultPayload { ok: [] }, tag: FsCreateDirAtResultTag::Ok },
            Err(e) => FsCreateDirAtResult { payload: FsCreateDirAtResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: FsCreateDirAtResultTag::Err },
        }
    }

    pub fn write_file_at(r: &Root, d: *mut u64, path: RocListWith<u8, false>, data: RocListWith<u8, false>) -> FsWriteFileAtResult {
        let v = bytes(&data); unsafe { data.decref(abi::host()) };
        unit(with_path(r, d, path, |p| std::fs::write(p, &v)))
    }

    pub fn stat_at(r: &Root, d: *mut u64, path: RocListWith<u8, false>) -> FsStatAtResult {
        match with_path(r, d, path, |p| std::fs::symlink_metadata(p)) {
            Ok(m) => {
                let ft = m.file_type();
                let kind = if ft.is_dir() { DirOrFileOrOtherOrSymLink::Dir } else if ft.is_file() { DirOrFileOrOtherOrSymLink::File } else if ft.is_symlink() { DirOrFileOrOtherOrSymLink::SymLink } else { DirOrFileOrOtherOrSymLink::Other };
                let mode = m.permissions().mode();
                let rec = AnonStruct3cd788af8f271d34 {
                    modified_ns: m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as u64).unwrap_or(0),
                    size: m.len(),
                    executable: mode & 0o111 != 0,
                    kind,
                    readable: mode & 0o444 != 0,
                    writable: !m.permissions().readonly(),
                };
                FsStatAtResult { payload: FsStatAtResultPayload { ok: ManuallyDrop::new(rec) }, tag: FsStatAtResultTag::Ok }
            }
            Err(e) => FsStatAtResult { payload: FsStatAtResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: FsStatAtResultTag::Err },
        }
    }

    pub fn read_dir_at(r: &Root, d: *mut u64, path: RocListWith<u8, false>) -> FsReadDirAtResult {
        let res = with_path(r, d, path, |p| {
            let mut out = Vec::new();
            let mut names: Vec<Vec<u8>> = std::fs::read_dir(p)?.map(|e| e.map(|e| e.file_name().as_bytes().to_vec())).collect::<std::io::Result<_>>()?;
            names.sort();
            for (i, n) in names.iter().enumerate() {
                if i > 0 { out.push(0); }
                out.extend_from_slice(n);
            }
            Ok(out)
        });
        match res {
            Ok(v) => FsReadDirAtResult { payload: FsReadDirAtResultPayload { ok: ManuallyDrop::new(unsafe { RocListWith::<u8, false>::from_slice(&v, abi::host()) }) }, tag: FsReadDirAtResultTag::Ok },
            Err(e) => FsReadDirAtResult { payload: FsReadDirAtResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: FsReadDirAtResultTag::Err },
        }
    }

    pub fn create_dir_at(r: &Root, d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { dir_unit(with_path(r, d, p, std::fs::create_dir)) }
    pub fn create_dir_all_at(r: &Root, d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { dir_unit(with_path(r, d, p, std::fs::create_dir_all)) }
    pub fn remove_dir_at(r: &Root, d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { dir_unit(with_path(r, d, p, std::fs::remove_dir)) }
    pub fn remove_dir_all_at(r: &Root, d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { dir_unit(with_path(r, d, p, std::fs::remove_dir_all)) }
    pub fn unlink_at(r: &Root, d: *mut u64, p: RocListWith<u8, false>) -> FsWriteFileAtResult { unit(with_path(r, d, p, std::fs::remove_file)) }

    fn two(r: &Root, d: *mut u64, a: RocListWith<u8, false>, b: RocListWith<u8, false>, f: impl FnOnce(PathBuf, PathBuf) -> std::io::Result<()>) -> FsWriteFileAtResult {
        let ra = bytes(&a); unsafe { a.decref(abi::host()) };
        let rb = bytes(&b); unsafe { b.decref(abi::host()) };
        let dir = unsafe { abi::resource::with(d as RocBox, |x: &mut Desc| dir_of(x)) };
        unit(resolve(r, &dir, &ra).and_then(|pa| resolve(r, &dir, &rb).and_then(|pb| f(pa, pb))))
    }
    pub fn rename_at(r: &Root, d: *mut u64, a: RocListWith<u8, false>, b: RocListWith<u8, false>) -> FsWriteFileAtResult { two(r, d, a, b, |a, b| std::fs::rename(a, b)) }
    pub fn link_at(r: &Root, d: *mut u64, a: RocListWith<u8, false>, b: RocListWith<u8, false>) -> FsWriteFileAtResult { two(r, d, a, b, |a, b| std::fs::hard_link(a, b)) }

    pub fn live(_r: &Root) -> i32 { abi::resource::live() as i32 }
}

/// Emit the 16 hosted symbols for a root policy under `$prefix`
/// (`trantor__<prefix>__<op>`). Invoked once per thin staticlib.
#[macro_export]
macro_rules! exports {
    ($prefix:ident, $root:expr) => {
        $crate::paste::paste! {
            static ROOT: std::sync::OnceLock<$crate::Root> = std::sync::OnceLock::new();
            fn root() -> &'static $crate::Root { ROOT.get_or_init(|| $root) }
            use trantor_abi::*;
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __preopen_count>]() -> u64 { $crate::ops::preopen_count(root()) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __preopen_at>](i: u64) -> *mut u64 { $crate::ops::preopen_at(root(), i) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __open_at>](d: *mut u64, p: RocListWith<u8, false>, f: u8) -> FsOpenAtResult { $crate::ops::open_at(root(), d, p, f) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __read_via_stream>](d: *mut u64) -> *mut u64 { $crate::ops::read_via_stream(root(), d) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __read_file_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsReadFileAtResult { $crate::ops::read_file_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __write_file_at>](d: *mut u64, p: RocListWith<u8, false>, b: RocListWith<u8, false>) -> FsWriteFileAtResult { $crate::ops::write_file_at(root(), d, p, b) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __stat_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsStatAtResult { $crate::ops::stat_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __read_dir_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsReadDirAtResult { $crate::ops::read_dir_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __create_dir_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { $crate::ops::create_dir_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __create_dir_all_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { $crate::ops::create_dir_all_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __remove_dir_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { $crate::ops::remove_dir_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __remove_dir_all_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsCreateDirAtResult { $crate::ops::remove_dir_all_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __unlink_at>](d: *mut u64, p: RocListWith<u8, false>) -> FsWriteFileAtResult { $crate::ops::unlink_at(root(), d, p) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __rename_at>](d: *mut u64, a: RocListWith<u8, false>, b: RocListWith<u8, false>) -> FsWriteFileAtResult { $crate::ops::rename_at(root(), d, a, b) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __link_at>](d: *mut u64, a: RocListWith<u8, false>, b: RocListWith<u8, false>) -> FsWriteFileAtResult { $crate::ops::link_at(root(), d, a, b) }
            #[unsafe(no_mangle)] pub extern "C-unwind" fn [<trantor__ $prefix __live>]() -> i32 { $crate::ops::live(root()) }
        }
    };
}
