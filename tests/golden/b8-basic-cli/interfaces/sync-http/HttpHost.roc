## roc:sync-http: one blocking outgoing request. The request record restates
## basic-cli's InternalHttp.RequestToAndFromHost structurally (records unify),
## so this interface stays independent of the derived layer. Response headers
## come back NUL-joined (name\0value\0name\0value...) because a host-built
## List((Str, Str)) is the R-B5 gap; the Host shim splits them in Roc.
HttpHost :: [].{
	TransportErr : [Timeout, NetworkError, BadBody, Other(List(U8))]
	Request : { method : U8, method_ext : Str, headers : List((Str, Str)), uri : Str, body : List(U8), timeout_ms : U64 }
	Response : { status : U16, headers_flat : List(U8), body : List(U8) }
	send! : Request => Try(Response, TransportErr)
}
