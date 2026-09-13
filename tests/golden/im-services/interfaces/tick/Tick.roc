## An asynchronous service's command union: `Start(request_id, route_key, n)`
## spawns a thread that wakes the driver `n` times; each wake routes one
## `TickEvent.Ticked` (roc-solid's `dbx`/`net` shape). `TickEnv` is its
## ambient block in `Env` (roc-solid's `audio` shape).
Tick := [
	Start(U64, Str, U64),
	Stop,
]
