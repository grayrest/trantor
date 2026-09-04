import Streams
## Test backing: an InputStream over an in-memory byte buffer.
Memory :: [].{
	open! : List(U8) => Streams.InputStream
	## Given the memory byte count, returns it as the exit code if every stream
	## resource has been dropped (live == 0), else -1. One number checks both.
	report! : U64 => I32
}
