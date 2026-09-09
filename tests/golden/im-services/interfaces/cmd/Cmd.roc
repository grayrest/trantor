## The composed command union: the driver's core variants plus one wrapper per
## service component (hand-written here; P1 generates it from the
## `## @hematite(cmd)` splice in the driver's Cmd.roc).
import Echo
import Tick

Cmd := [
	Log(Str),
	Echo(Echo),
	Tick(Tick),
]
