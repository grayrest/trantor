## The per-frame ambient record. A service with an env block gets a field here
## at the marker; a nominal record pattern must name every field (measured), so
## the driver's own methods destructure with full patterns.
Env := {
	width : U64,
	## @trantor(env)
	## @end
}.{
	width : Env -> U64
	width = |env| {
		Env.{ width: w, tick: _ } = env
		w
	}

	ticks : Env -> U64
	ticks = |env| {
		Env.{ width: _, tick: t } = env
		TickEnv.ticks(t)
	}
}
