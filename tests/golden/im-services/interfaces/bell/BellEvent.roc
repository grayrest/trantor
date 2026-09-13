## One event with several fields, beside a several-field command: the repair
## must retype the Event union's `bell` and leave the Cmd union's alone.
BellEvent := [
	Rang(Str, U64),
]
