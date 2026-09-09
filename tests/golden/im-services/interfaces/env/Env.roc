## The per-frame ambient record, with a component-owned block (`tick`) — the
## shape P1's `## @hematite(env)` splice produces in the real Env.roc.
import TickEnv

Env := { width : U64, tick : TickEnv }.{
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
