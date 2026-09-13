## A service with ONE command, and it has several fields. Glue unwraps a
## single-variant union and types its payload as the first field's type (`Str`
## here); composition repairs that (D-H7-44), so the service keeps its one
## natural command. The comment after the last variant is part of the test.
Bell := [
	Ring(Str, U64, Str) # route key, number, text
]
