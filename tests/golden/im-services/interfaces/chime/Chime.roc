## One command whose one field is a record. Glue names the unwrapped union after
## the record's `AnonStruct` itself, so composition must not alias it again
## (D-H7-45).
Chime := [
	Strike({ key : Str, n : U64 }),
]
