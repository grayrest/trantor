app [main!] { pf: platform "../target/hematite/b6-net/platform/main.roc" }
import pf.Stdout
import pf.Streams
import pf.TempTest

## HC4: an https:// GET against the local rustls testnet, trusting its cert via
## HEMATITE_HTTP_EXTRA_CA. With the tls feature ON the handshake succeeds and the
## body streams over TLS ("https: 200 https-hello"); with tls OFF the primitive
## rejects https:// up front ("https: other").
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	port = TempTest.start_https_server!({})
	if port == 0 {
		Stdout.line!("https: no-cert") ?? {}
		Err(Exit(1))
	} else {
		uri = Str.concat("https://localhost:", Str.concat(u16_str(port), "/"))
		result = match TempTest.send!({ method: 3, method_ext: "", headers: [], uri, body: [], timeout_ms: 5000 }) {
			Ok(resp) => {
				body = collect!(resp.body_stream, [])
				Str.concat(u16_str(resp.status), Str.concat(" ", Str.from_utf8_lossy(body)))
			}
			Err(Timeout) => "timeout"
			Err(NetworkError) => "network"
			Err(BadBody) => "badbody"
			Err(Other(_)) => "other"
		}
		Stdout.line!(Str.concat("https: ", result)) ?? {}
		Err(Exit(0))
	}
}

collect! : Streams.InputStream, List(U8) => List(U8)
collect! = |stream, acc| {
	match Streams.read!(stream, 65536) {
		Ok(chunk) => if List.is_empty(chunk) { acc } else { collect!(stream, List.concat(acc, chunk)) }
		Err(_) => acc
	}
}

u16_str : U16 -> Str
u16_str = |n| if n == 0 { "0" } else { go(n, "") }
go : U16, Str -> Str
go = |n, acc| if n == 0 { acc } else { go(n // 10, Str.concat(digit(n % 10), acc)) }
digit : U16 -> Str
digit = |d| {
	match d {
		0 => "0"
		1 => "1"
		2 => "2"
		3 => "3"
		4 => "4"
		5 => "5"
		6 => "6"
		7 => "7"
		8 => "8"
		_ => "9"
	}
}
