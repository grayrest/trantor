import IOErr exposing [IOErr]
import Streams
## Test backing: an InputStream over a real file (stand-in for B3's
## descriptor.read-via-stream).
FileIo :: [].{
	open! : Str => Try(Streams.InputStream, [FileErr(IOErr)])
}
