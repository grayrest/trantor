## memfs: a pure-Roc shim that fulfills the `filesystem` interface (D19) with an
## in-memory table -- no host archive of its own. It imports `cell` and does a
## put!/get! round-trip on every read: the read only succeeds if the value
## stored through the host comes back intact, so a successful "3 lines" run also
## proves Roc-shim state through `cell` (D12). This is the SAME interface
## capstdfs implements in Rust; the derived `Path` component is byte-identical
## between the two wirings.
import IOErr exposing [IOErr]
import Cell
Fs :: [].{
	file_read! : Str => Try(Str, [FileErr(IOErr)])
	file_read! = |path| {
		Cell.put!(path)               # store through the host
		echoed = Cell.get!({})        # read back through the host
		if !Str.is_eq(echoed, path) {
			Err(FileErr(IOErr.Other("cell round-trip failed")))
		} else {
			match lookup(path) {
				Ok(contents) => Ok(contents)
				Err({}) => Err(FileErr(IOErr.NotFound))
			}
		}
	}
}

lookup : Str -> Try(Str, {})
lookup = |path| {
	match path {
		"hello.txt" => Ok("line one\nline two\nline three\n")
		"empty.txt" => Ok("")
		"one.txt" => Ok("just one line\n")
		_ => Err({})
	}
}
