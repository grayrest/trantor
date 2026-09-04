import IOErr exposing [IOErr]
import Fs
import Cell
import CliEnv
import Streams
## The shared List(U8) core beneath roc:path and roc:os-path (P11). Paths
## resolve against the userland cwd (a Cell prefix; empty = the process cwd),
## then run against preopen 0 -- basic-cli's ambient authority re-expressed as
## a capability the world granted (P4/P8).
FsOps :: [].{
	cwd! : {} => Str
	cwd! = |{}| {
		c = Cell.get!({})
		if Str.is_empty(c) { CliEnv.cwd!({}) } else { c }
	}
	set_cwd! : Str => {}
	set_cwd! = |p| Cell.put!(p)

	resolve! : List(U8) => List(U8)
	resolve! = |p| if is_absolute(p) { p } else { join_bytes(Str.to_utf8(cwd!({})), p) }

	read! : List(U8) => Try(List(U8), [FileErr(IOErr)])
	read! = |p| Fs.read_file_at!(Fs.preopen_at!(0), resolve!(p))
	write! : List(U8), List(U8) => Try({}, [FileErr(IOErr)])
	write! = |p, b| Fs.write_file_at!(Fs.preopen_at!(0), resolve!(p), b)
	delete! : List(U8) => Try({}, [FileErr(IOErr)])
	delete! = |p| Fs.unlink_at!(Fs.preopen_at!(0), resolve!(p))
	kind! : List(U8) => Try([File, Dir, SymLink, Other], IOErr)
	kind! = |p| {
		match Fs.stat_at!(Fs.preopen_at!(0), resolve!(p)) {
			Ok(s) => Ok(s.kind)
			Err(e) => Err(e)
		}
	}
	size! : List(U8) => Try(U64, IOErr)
	size! = |p| {
		match Fs.stat_at!(Fs.preopen_at!(0), resolve!(p)) {
			Ok(s) => Ok(s.size)
			Err(e) => Err(e)
		}
	}
	executable! : List(U8) => Try(Bool, IOErr)
	executable! = |p| {
		match Fs.stat_at!(Fs.preopen_at!(0), resolve!(p)) {
			Ok(s) => Ok(s.executable)
			Err(e) => Err(e)
		}
	}
	list! : List(U8) => Try(List(List(U8)), [DirErr(IOErr)])
	list! = |p| {
		match Fs.read_dir_at!(Fs.preopen_at!(0), resolve!(p)) {
			Ok(joined) => Ok(split_nul(joined))
			Err(e) => Err(e)
		}
	}
	create_dir! : List(U8) => Try({}, [DirErr(IOErr)])
	create_dir! = |p| Fs.create_dir_at!(Fs.preopen_at!(0), resolve!(p))
	create_all! : List(U8) => Try({}, [DirErr(IOErr)])
	create_all! = |p| Fs.create_dir_all_at!(Fs.preopen_at!(0), resolve!(p))
	delete_empty! : List(U8) => Try({}, [DirErr(IOErr)])
	delete_empty! = |p| Fs.remove_dir_at!(Fs.preopen_at!(0), resolve!(p))
	delete_all! : List(U8) => Try({}, [DirErr(IOErr)])
	delete_all! = |p| Fs.remove_dir_all_at!(Fs.preopen_at!(0), resolve!(p))
	rename! : List(U8), List(U8) => Try({}, [FileErr(IOErr)])
	rename! = |a, b| Fs.rename_at!(Fs.preopen_at!(0), resolve!(a), resolve!(b))
	hard_link! : List(U8), List(U8) => Try({}, [FileErr(IOErr)])
	hard_link! = |a, b| Fs.link_at!(Fs.preopen_at!(0), resolve!(a), resolve!(b))
	readable! : List(U8) => Try(Bool, IOErr)
	readable! = |p| stat_field!(p, |s| s.readable)
	writable! : List(U8) => Try(Bool, IOErr)
	writable! = |p| stat_field!(p, |s| s.writable)
	accessed! : List(U8) => Try(U128, IOErr)
	accessed! = |p| stat_field!(p, |s| s.accessed_ns)
	modified! : List(U8) => Try(U128, IOErr)
	modified! = |p| stat_field!(p, |s| s.modified_ns)
	created! : List(U8) => Try(U128, IOErr)
	created! = |p| stat_field!(p, |s| s.created_ns)
	## Open for buffered reading: a sync-io stream over the descriptor.
	open_read! : List(U8) => Try(Streams.InputStream, [FileErr(IOErr)])
	open_read! = |p| {
		match Fs.open_at!(Fs.preopen_at!(0), resolve!(p), 0) {
			Ok(d) => Ok(Fs.read_via_stream!(d))
			Err(e) => Err(e)
		}
	}
	live! : {} => I32
	live! = |{}| Fs.live!({})
}

is_absolute : List(U8) -> Bool
is_absolute = |p| {
	match List.first(p) {
		Ok(c) => c == '/'
		Err(_) => False
	}
}

join_bytes : List(U8), List(U8) -> List(U8)
join_bytes = |a, b| {
	match List.last(a) {
		Ok(c) => if c == '/' { List.concat(a, b) } else { List.concat(List.append(a, '/'), b) }
		Err(_) => b
	}
}

## Split a NUL-joined listing into names (filenames never contain NUL).
split_nul : List(U8) -> List(List(U8))
split_nul = |bytes| {
	st = List.fold(bytes, { acc: [], cur: [] }, |s, b| if b == 0 { { acc: List.append(s.acc, s.cur), cur: [] } } else { { acc: s.acc, cur: List.append(s.cur, b) } })
	if List.is_empty(st.cur) { st.acc } else { List.append(st.acc, st.cur) }
}

stat_field! : List(U8), ({ kind : [File, Dir, SymLink, Other], size : U64, accessed_ns : U128, modified_ns : U128, created_ns : U128, readable : Bool, writable : Bool, executable : Bool } -> a) => Try(a, IOErr)
stat_field! = |p, pick| {
	match Fs.stat_at!(Fs.preopen_at!(0), FsOps.resolve!(p)) {
		Ok(s) => Ok(pick(s))
		Err(e) => Err(e)
	}
}
