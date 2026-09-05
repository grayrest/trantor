## SQ0 substrate probes for roc:sqlite-unsound.
Probe :: [].{
	## Returns a Str whose bytes are BORROWED from a host-owned static buffer
	## (zero-copy, rc==0 immortal). The caller must consume it in place.
	borrow_str! : {} => Str
	## Invokes a boxed Roc closure `(I64 -> I64)` from the host and returns its
	## result — the erased-callable path the fold reducer and turso scalar ride.
	apply_i64! : I64, Box((I64 -> I64)) => I64
	## Print a line host-side (avoids needing a stdio host in this micro-fixture).
	print! : Str => {}
}
