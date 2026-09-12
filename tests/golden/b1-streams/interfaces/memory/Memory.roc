import Streams
## Test backing: an InputStream over an in-memory byte buffer.
Memory :: [].{
	open! : List(U8) => Streams.InputStream
	## Given what the app read from each backing, returns the memory count as the
	## exit code if every stream resource has been dropped (live == 0), else -1,
	## and prints both counts for the gate.
	##
	## The FILE count used to be discarded in the app (`f_len * 0`) and verified
	## only by a debug `eprintln!` in a vendored copy of the sync-io host. When
	## B1 moved onto trantor-cli's real sync-io, that print went with the copy —
	## and with it the only check on the file backing, which is half of what B1
	## exists to show. It comes from what the app actually observed now.
	report! : U64, U64 => I32
}
