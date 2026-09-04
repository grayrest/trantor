## roc:sync-http: one blocking outgoing request. The request record restates
## basic-cli's InternalHttp.RequestToAndFromHost structurally (records unify),
## so this interface stays independent of the derived layer. Response headers
## come back NUL-joined (name\0value\0name\0value...) because a host-built
## List((Str, Str)) is the R-B5 gap; the Host shim splits them in Roc.
##
## The response BODY is a streaming `Streams.InputStream` (H5, WASI's
## incoming-response -> incoming-body -> input-stream), the same refcounted
## resource files/sockets/stdin produce. `send!` returns as soon as the final
## headers arrive; the caller reads the body stream (or collects it). Body-phase
## failures (reset, truncation, decompression) surface as StreamErr on read, not
## as a send! error (H15).
import Streams
HttpHost :: [].{
	TransportErr : [Timeout, NetworkError, BadBody, Other(List(U8))]
	Request : { method : U8, method_ext : Str, headers : List((Str, Str)), uri : Str, body : List(U8), timeout_ms : U64 }
	Response : { status : U16, headers_flat : List(U8), body_stream : Streams.InputStream }
	send! : Request => Try(Response, TransportErr)
}
