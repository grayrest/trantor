## Fetch UTF-8, JSON, and HTML responses from a local HTTP server.
app [main!] {
	pf: platform "../target/trantor/b8-basic-cli/platform/main.roc",
	http: "https://github.com/roc-lang/http/releases/download/1.0.0/6ZUwqYhCS8PU9Mo6MF7oV82ET2o7KYb57CLKDq4cq4sS.tar.zst",
}

import pf.OsStr
import pf.Http
import pf.Stdout
import pf.TempTest
import http.Request

main! : List(OsStr) => Try({}, _)
main! = |_args| {
	TempTest.start_test_server!({})

	hello_str = Http.get_utf8!("http://127.0.0.1:9000/utf8test") ? |err| GetUtf8Failed(err)
	Stdout.line!("I received '${hello_str}' from the server.")?

	decoded : { foo : Str }
	decoded = Http.get!("http://127.0.0.1:9000") ? |err| GetJsonFailed(err)

	Stdout.line!("The json I received was: { foo: \"${decoded.foo}\" }")?

	response = Http.send!(Request.from_method(GET).with_uri("http://127.0.0.1:9000/html")) ? |err| SendHtmlFailed(err)
	body = Str.from_utf8(Http.read_body_to_end!(response)) ? |err| HtmlBodyUtf8Failed(err)

	Stdout.line!("Response body:")?
	Stdout.line!(body)?

	Ok({})
}
