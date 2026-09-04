Env := { width : U64 }.{
	width : Env -> U64
	width = |env| {
		Env.{ width: w } = env
		w
	}
}
