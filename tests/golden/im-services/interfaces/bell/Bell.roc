## A service with ONE command, and it has several fields: `Ring(request_id,
## route_key, text)`. Glue unwraps a single-variant union and mis-types a
## multi-field payload; composition repairs that (D-H7-44), so the service keeps
## its one natural command.
Bell := [
	Ring(U64, Str, Str),
]
