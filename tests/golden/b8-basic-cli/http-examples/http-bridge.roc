## HC3: prove Http.to_http_response! round-trips a streaming response into the
## eager roc-lang/http `Response` — the bridged value is read back with that
## package's own `Response.status`/`Response.body` accessors.
app [main!] {
	pf: platform "../platform/main.roc",
	http: "https://github.com/roc-lang/http/releases/download/1.0.0/6ZUwqYhCS8PU9Mo6MF7oV82ET2o7KYb57CLKDq4cq4sS.tar.zst",
}

import pf.OsStr
import pf.Http
import pf.Stdout
import pf.TempTest
import http.Request
import http.Response

main! : List(OsStr) => Try({}, _)
main! = |_args| {
	TempTest.start_test_server!({})
	response = Http.send!(Request.from_method(GET).with_uri("http://127.0.0.1:9000/utf8test")) ? |err| SendFailed(err)
	http_response = Http.to_http_response!(response)
	status = U16.to_str(Response.status(http_response))
	body = Str.from_utf8(Response.body(http_response)) ? |_| BridgeBodyUtf8Failed
	Stdout.line!("bridge: ${status} ${body}")?
	Ok({})
}
