import IOErr exposing [IOErr]
## roc:sync-io/streams: the unified blocking stream substrate. File reads,
## socket reads and stdin all yield an InputStream; stdout/sockets take an
## OutputStream. Both are resources (refcounted opaque host handles, P5).
Streams :: [].{
	InputStream :: Box(U64)
	OutputStream :: Box(U64)
	## Read up to `max` bytes. Empty list = end of stream.
	read! : InputStream, U64 => Try(List(U8), [StreamErr(IOErr)])
	write! : OutputStream, List(U8) => Try({}, [StreamErr(IOErr)])
}
