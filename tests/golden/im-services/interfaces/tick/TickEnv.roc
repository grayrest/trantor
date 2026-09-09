TickEnv := { ticks : U64 }.{
	ticks : TickEnv -> U64
	ticks = |e| {
		TickEnv.{ ticks: t } = e
		t
	}
}
