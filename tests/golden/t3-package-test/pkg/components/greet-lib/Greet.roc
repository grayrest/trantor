Greet :: [].{
	hello : Str -> Str
	hello = |name| "hello ${name}"
}

expect Greet.hello("x") == "hello x"
