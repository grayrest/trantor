app [run!] { pf: platform "../platform/main.roc" }
import pf.Streams
import pf.Memory
import pf.FileIo

## Reads a memory-backed and a file-backed stream through the SAME
## InputStream resource. Exit code == memory bytes (11) iff every stream
## dropped (live == 0); -1 (255) on a leak.
run! : {} => Try({}, [Exit(I32), ..])
run! = |{}| {
	mem_len = read_all!({})
	Err(Exit(Memory.report!(mem_len)))
}

read_all! : {} => U64
read_all! = |{}| {
	m = Memory.open!(Str.to_utf8("hello world"))
	chunk = Streams.read!(m, 1024) ?? []
	f_len = read_file!({})
	List.len(chunk) + (f_len * 0)     # f_len is checked host-side; keep it live here
}

read_file! : {} => U64
read_file! = |{}| {
	match FileIo.open!("world.toml") {
		Ok(f) => {
			bytes = Streams.read!(f, 4096) ?? []
			List.len(bytes)
		}
		Err(_) => 0
	}
}
