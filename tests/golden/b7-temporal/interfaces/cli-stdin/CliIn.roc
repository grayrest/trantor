import IOErr exposing [IOErr]
import Streams
CliIn :: [].{
	get_stdin! : {} => Streams.InputStream
	## One line without its newline; EndOfFile at EOF. Host-buffered.
	read_line! : {} => Try(Str, [EndOfFile, StdinErr(IOErr)])
	read_to_end! : {} => Try(List(U8), [StdinErr(IOErr)])
}
