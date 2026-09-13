## A synchronous service's command union: every `Ping` is answered with one
## `EchoEvent.Pong` from inside the same drain (roc-solid's `notes` shape).
## One nominal per module (measured: a type module exposes only the nominal
## named after the file), so the events live in `EchoEvent.roc`.
##
## Two variants: glue's own named-union path. One-variant unions are Bell's and
## Nudge's (D-H7-44).
Echo := [
	## `Ping(request_id, route_key, text)`
	Ping(U64, Str, Str),
	Shout(Str),
]
