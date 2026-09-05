app [main!] { pf: platform "../platform/main.roc" }

import pf.Probe

## SQ0 go/no-go: prove the two roc:sqlite-unsound mechanisms on hematite's stack.
main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	# (A) Borrowed slice: use the borrow TWICE, which makes Roc incref it once and
	# decref it twice. The rc==0 static backing is read-only memory, so an errant
	# incref/decref that WROTE the count would segfault; copying out via concat
	# also exercises reading the borrowed bytes. Surviving proves rc==0 immortal.
	borrowed = Probe.borrow_str!({})
	Probe.print!(Str.concat(borrowed, borrowed))
	# (B) Erased callable: the host invokes this boxed Roc closure.
	n = Probe.apply_i64!(21, Box.box(double))
	Probe.print!(I64.to_str(n))
	Ok({})
}

double : I64 -> I64
double = |x| x + x
