import Streams
## roc:cli/stdout + roc:cli/stderr primitives: hand out the process streams.
CliOut :: [].{
	get_stdout! : {} => Streams.OutputStream
	get_stderr! : {} => Streams.OutputStream
}
