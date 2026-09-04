app [run!] { pf: platform "../platform/main.roc" }
import pf.Gauge exposing [Counter]

## All counter use lives in this helper so every Counter is unreachable (and
## therefore dropped by refcount) before `report!` runs.
use_counters! : {} => U64
use_counters! = |{}| {
	a = Gauge.open!({})
	b = Gauge.open!({})
	c = Gauge.open!({})
	x = Gauge.bump!(a)
	y = Gauge.bump!(b)
	z = Gauge.bump!(c)
	w = peek(b)            # a borrow: passes b by value (refcount bump), must NOT close it
	y2 = Gauge.bump!(b)    # b still alive after the borrow
	x + y + z + w + y2
}

peek : Counter -> U64
peek = |_c| 1

run! : {} => Try({}, [Exit(I32), ..])
run! = |{}| {
	_total = use_counters!({})
	closes = Gauge.report!({})
	Err(Exit(closes))   # exit code == closes observed (expect 3)
}
