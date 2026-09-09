## A synchronous service's command union: every `Ping` is answered with one
## `EchoEvent.Pong` from inside the same drain (roc-solid's `notes` shape).
## One nominal per module (measured: a type module exposes only the nominal
## named after the file), so the events live in `EchoEvent.roc`.
##
## Two variants, deliberately (measured, P0): glue unwraps a SINGLE-variant
## union to its payload and mis-types a multi-field one as `u64` (its own size
## assert then fails the build). With two or more it emits a named `Echo` type.
Echo := [
	## `Ping(request_id, route_key, text)`
	Ping(U64, Str, Str),
	Shout(Str),
]
