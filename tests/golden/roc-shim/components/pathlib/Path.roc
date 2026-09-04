## pathlib: a pure-Roc derived component over the `fs` interface. No host
## archive, no hosted symbols. `line_count!` is derived logic (the kind that
## would drift if every fs implementation shipped its own copy).
import Fs
import IOErr exposing [IOErr]
Path :: [].{
	read_to_string! : Str => Try(Str, [FileErr(IOErr)])
	read_to_string! = |p| Fs.file_read!(p)

	line_count! : Str => Try(U64, [FileErr(IOErr)])
	line_count! = |p| {
		text = Fs.file_read!(p)?
		Ok(count_lines(text))
	}
}

count_lines : Str -> U64
count_lines = |text| {
	List.fold(Str.to_utf8(text), 0, |acc, b| if b == '\n' { acc + 1 } else { acc })
}
