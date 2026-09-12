app [run!] { pf: platform "../target/trantor/b1-streams/platform/main.roc" }
import pf.Streams
import pf.Memory
import pf.FileIo

## Reads a memory-backed and a file-backed stream through the SAME
## InputStream resource. Exit code == memory bytes (11) iff every stream
## dropped (live == 0); -1 (255) on a leak.
run! : {} => Try({}, [Exit(I32), ..])
run! = |{}| {
	m = Memory.open!(Str.to_utf8("hello world"))
	mem_len = List.len(Streams.read!(m, 1024) ?? [])
	file_len = read_file!({})
	Err(Exit(Memory.report!(mem_len, file_len)))
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
