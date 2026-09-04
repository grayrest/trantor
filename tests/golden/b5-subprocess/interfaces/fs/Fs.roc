import IOErr exposing [IOErr]
import Streams
## roc:filesystem primitives: WASI's capability model. Every op is relative to a
## Descriptor (a preopened directory or an opened file); there is no ambient
## path authority at this layer (P4). Paths are raw bytes (P11).
Fs :: [].{
	Descriptor :: Box(U64)
	preopen_count! : {} => U64
	preopen_at! : U64 => Descriptor
	## flags: 0 = read, 1 = write (create + truncate)
	open_at! : Descriptor, List(U8), U8 => Try(Descriptor, [FileErr(IOErr)])
	read_via_stream! : Descriptor => Streams.InputStream
	read_file_at! : Descriptor, List(U8) => Try(List(U8), [FileErr(IOErr)])
	write_file_at! : Descriptor, List(U8), List(U8) => Try({}, [FileErr(IOErr)])
	stat_at! : Descriptor, List(U8) => Try({ kind : [File, Dir, SymLink, Other], size : U64, modified_ns : U64, readable : Bool, writable : Bool, executable : Bool }, IOErr)
	## NUL-joined entry names.
	read_dir_at! : Descriptor, List(U8) => Try(List(U8), [DirErr(IOErr)])
	create_dir_at! : Descriptor, List(U8) => Try({}, [DirErr(IOErr)])
	create_dir_all_at! : Descriptor, List(U8) => Try({}, [DirErr(IOErr)])
	remove_dir_at! : Descriptor, List(U8) => Try({}, [DirErr(IOErr)])
	remove_dir_all_at! : Descriptor, List(U8) => Try({}, [DirErr(IOErr)])
	unlink_at! : Descriptor, List(U8) => Try({}, [FileErr(IOErr)])
	rename_at! : Descriptor, List(U8), List(U8) => Try({}, [FileErr(IOErr)])
	link_at! : Descriptor, List(U8), List(U8) => Try({}, [FileErr(IOErr)])
	## Live resource count (drop-balance gauge); exit-code sized.
	live! : {} => I32
}
