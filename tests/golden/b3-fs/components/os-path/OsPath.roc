import IOErr exposing [IOErr]
import FsOps
## roc:os-path: the LOSSLESS path (P11) -- [Text(Str) | Raw(List(U8))]. Same
## function names as roc:path, so a world may expose it AS `Path` (D14 rename)
## and app code compiles unchanged. Raw bytes round-trip; `to_str` is lossy on Raw.
OsPath := [Text(Str), Raw(List(U8))].{
	from_str : Str -> OsPath
	from_str = |s| OsPath.Text(s)
	from_raw : List(U8) -> OsPath
	from_raw = |b| OsPath.Raw(b)
	to_str : OsPath -> Str
	to_str = |p| {
		match p {
			OsPath.Text(s) => s
			OsPath.Raw(b) => Str.from_utf8_lossy(b)
		}
	}
	display : OsPath -> Str
	display = |p| to_str(p)
	join : OsPath, Str -> OsPath
	join = |p, seg| OsPath.Raw(join_bytes(to_bytes(p), Str.to_utf8(seg)))
	to_bytes : OsPath -> List(U8)
	to_bytes = |p| {
		match p {
			OsPath.Text(s) => Str.to_utf8(s)
			OsPath.Raw(b) => b
		}
	}

	read_utf8! : OsPath => Try(Str, [FileErr(IOErr), BadUtf8])
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
	read_bytes! : OsPath => Try(List(U8), [FileErr(IOErr)])
	read_bytes! = |p| FsOps.read!(to_bytes(p))
	write_utf8! : OsPath, Str => Try({}, [FileErr(IOErr)])
	write_utf8! = |p, s| FsOps.write!(to_bytes(p), Str.to_utf8(s))
	write_bytes! : OsPath, List(U8) => Try({}, [FileErr(IOErr)])
	write_bytes! = |p, b| FsOps.write!(to_bytes(p), b)
	delete! : OsPath => Try({}, [FileErr(IOErr)])
	delete! = |p| FsOps.delete!(to_bytes(p))
	type! : OsPath => Try([IsFile, IsDir, IsSymLink, IsOther], IOErr)
	type! = |p| {
		match FsOps.kind!(to_bytes(p)) {
			Ok(File) => Ok(IsFile)
			Ok(Dir) => Ok(IsDir)
			Ok(SymLink) => Ok(IsSymLink)
			Ok(Other) => Ok(IsOther)
			Err(e) => Err(e)
		}
	}
	is_file! : OsPath => Try(Bool, IOErr)
	is_file! = |p| {
		match type!(p) {
			Ok(IsFile) => Ok(True)
			Ok(_) => Ok(False)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	is_dir! : OsPath => Try(Bool, IOErr)
	is_dir! = |p| {
		match type!(p) {
			Ok(IsDir) => Ok(True)
			Ok(_) => Ok(False)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	exists! : OsPath => Try(Bool, IOErr)
	exists! = |p| {
		match type!(p) {
			Ok(_) => Ok(True)
			Err(NotFound) => Ok(False)
			Err(e) => Err(e)
		}
	}
	size_in_bytes! : OsPath => Try(U64, IOErr)
	size_in_bytes! = |p| FsOps.size!(to_bytes(p))
	## Lossless: a non-UTF-8 entry name is kept as Raw.
	list! : OsPath => Try(List(OsPath), [DirErr(IOErr)])
	list! = |p| {
		match FsOps.list!(to_bytes(p)) {
			Ok(names) => Ok(names.map(|n| to_entry(n)))
			Err(e) => Err(e)
		}
	}
	create_dir! : OsPath => Try({}, [DirErr(IOErr)])
	create_dir! = |p| FsOps.create_dir!(to_bytes(p))
	create_all! : OsPath => Try({}, [DirErr(IOErr)])
	create_all! = |p| FsOps.create_all!(to_bytes(p))
	delete_empty! : OsPath => Try({}, [DirErr(IOErr)])
	delete_empty! = |p| FsOps.delete_empty!(to_bytes(p))
	delete_all! : OsPath => Try({}, [DirErr(IOErr)])
	delete_all! = |p| FsOps.delete_all!(to_bytes(p))
	rename! : OsPath, OsPath => Try({}, [FileErr(IOErr)])
	rename! = |a, b| FsOps.rename!(to_bytes(a), to_bytes(b))
	hard_link! : OsPath, OsPath => Try({}, [FileErr(IOErr)])
	hard_link! = |a, b| FsOps.hard_link!(to_bytes(a), to_bytes(b))
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

to_entry : List(U8) -> OsPath
to_entry = |n| {
	match Str.from_utf8(n) {
		Ok(s) => OsPath.Text(s)
		Err(_) => OsPath.Raw(n)
	}
}
