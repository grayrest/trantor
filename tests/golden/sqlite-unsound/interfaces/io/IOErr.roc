## trantor-generated: vendored from interface roc:io/error@0.1.0.
## One nominal definition site shared by every interface that `use`s it.
IOErr := [
	NotFound,
	Other(Str),
].{
	to_str : IOErr -> Str
	to_str = |err| {
		match err {
			NotFound => "not found"
			Other(m) => m
		}
	}
}
