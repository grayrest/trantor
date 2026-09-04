import IOErr exposing [IOErr]
import FsOps
## roc:path: the Str-based path (P11). basic-cli's Path op surface, over the
## shared FsOps bytes core. A world may instead expose roc:os-path under the
## name `Path` (D14 rename) -- the two share every function name below.
Path := [Path(Str)].{
	from_str : Str -> Path
	from_str = |s| Path.Path(s)
	to_str : Path -> Str
	to_str = |p| {
		match p {
			Path.Path(s) => s
		}
	}
	display : Path -> Str
	display = |p| to_str(p)
	join : Path, Str -> Path
	join = |p, seg| Path.Path(Str.from_utf8_lossy(join_bytes(Str.to_utf8(to_str(p)), Str.to_utf8(seg))))
	to_bytes : Path -> List(U8)
	to_bytes = |p| Str.to_utf8(to_str(p))

	read_utf8! : Path => Try(Str, [FileErr(IOErr), BadUtf8])
	read_utf8! = |p| {
		match FsOps.read!(to_bytes(p)) {
			Ok(b) => {
				match Str.from_utf8(b) {
					Ok(s) => Ok(s)
					Err(_) => Err(BadUtf8)
				}
			}
			Err(FileErr(e)) => Err(FileErr(e))
		}
	}
	read_bytes! : Path => Try(List(U8), [FileErr(IOErr)])
	read_bytes! = |p| FsOps.read!(to_bytes(p))
	write_utf8! : Path, Str => Try({}, [FileErr(IOErr)])
	write_utf8! = |p, s| FsOps.write!(to_bytes(p), Str.to_utf8(s))
	write_bytes! : Path, List(U8) => Try({}, [FileErr(IOErr)])
	write_bytes! = |p, b| FsOps.write!(to_bytes(p), b)
	delete! : Path => Try({}, [FileErr(IOErr)])
	delete! = |p| FsOps.delete!(to_bytes(p))
	type! : Path => Try([IsFile, IsDir, IsSymLink, IsOther], IOErr)
	type! = |p| {
		match FsOps.kind!(to_bytes(p)) {
			Ok(File) => Ok(IsFile)
			Ok(Dir) => Ok(IsDir)
			Ok(SymLink) => Ok(IsSymLink)
			Ok(Other) => Ok(IsOther)
			Err(e) => Err(e)
		}
	}
	is_file! : Path => Try(Bool, IOErr)
	is_file! = |p| {
		match type!(p) {
			Ok(IsFile) => Ok(True)
			Ok(_) => Ok(False)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	is_dir! : Path => Try(Bool, IOErr)
	is_dir! = |p| {
		match type!(p) {
			Ok(IsDir) => Ok(True)
			Ok(_) => Ok(False)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	exists! : Path => Try(Bool, IOErr)
	exists! = |p| {
		match type!(p) {
			Ok(_) => Ok(True)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	size_in_bytes! : Path => Try(U64, IOErr)
	size_in_bytes! = |p| FsOps.size!(to_bytes(p))
	## Non-UTF-8 entry names are excluded (the Gleam/WASI model); use roc:os-path to keep them.
	list! : Path => Try(List(Path), [DirErr(IOErr)])
	list! = |p| {
		match FsOps.list!(to_bytes(p)) {
			Ok(names) => Ok(List.keep_oks(names, |n| Str.from_utf8(n)).map(|s| Path.Path(s)))
			Err(e) => Err(e)
		}
	}
	create_dir! : Path => Try({}, [DirErr(IOErr)])
	create_dir! = |p| FsOps.create_dir!(to_bytes(p))
	create_all! : Path => Try({}, [DirErr(IOErr)])
	create_all! = |p| FsOps.create_all!(to_bytes(p))
	delete_empty! : Path => Try({}, [DirErr(IOErr)])
	delete_empty! = |p| FsOps.delete_empty!(to_bytes(p))
	delete_all! : Path => Try({}, [DirErr(IOErr)])
	delete_all! = |p| FsOps.delete_all!(to_bytes(p))
	rename! : Path, Path => Try({}, [FileErr(IOErr)])
	rename! = |a, b| FsOps.rename!(to_bytes(a), to_bytes(b))
	hard_link! : Path, Path => Try({}, [FileErr(IOErr)])
	hard_link! = |a, b| FsOps.hard_link!(to_bytes(a), to_bytes(b))
	## seahaven Path constructors (Str-backed here; lossy for non-UTF-8 bytes on this package).
	utf8 : Str -> Path
	utf8 = |s| from_str(s)
	unix_bytes : List(U8) -> Path
	unix_bytes = |b| from_str(Str.from_utf8_lossy(b))
	windows_u16s : List(U16) -> Path
	windows_u16s = |_u| from_str("")
	is_executable! : Path => Try(Bool, IOErr)
	is_executable! = |p| {
		match FsOps.executable!(to_bytes(p)) {
			Ok(b) => Ok(b)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	is_eq : Path, Path -> Bool
	is_eq = |a, b| to_bytes(a) == to_bytes(b)
	to_hash : Path, Hasher -> Hasher
	to_hash = |p, hasher| Str.to_hash(to_str(p), hasher)
	live! : {} => I32
	live! = |{}| FsOps.live!({})
}

join_bytes : List(U8), List(U8) -> List(U8)
join_bytes = |a, b| {
	match List.last(a) {
		Ok(c) => if c == '/' { List.concat(a, b) } else { List.concat(List.append(a, '/'), b) }
		Err(_) => b
	}
}
