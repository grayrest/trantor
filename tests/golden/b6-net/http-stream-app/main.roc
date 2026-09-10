app [main!] { pf: platform "../target/hematite/b6-net/platform/main.roc" }
import pf.Stdout
import pf.Streams
import pf.Sockets
import pf.TempTest

## HC2: drive the streaming HTTP primitive directly — send!, then read the body
## InputStream in chunks. Exercises a large body (arrives whole), a redirect
## chain (followed unless HEMATITE_HTTP_MAX_REDIRECTS=0), multi-value headers
## (every occurrence preserved), a stall (send! -> Timeout), and a mid-body
## cutoff (StreamErr on read, H15). Exit code == live resources (0 = balanced).
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	port = TempTest.start_httpd!({})
	base = Str.concat("http://127.0.0.1:", u16_str(port))

	# Large body streamed in chunks; assert the whole thing arrives.
	large = send_and_drain!(Str.concat(base, "/large"), 5000)
	Stdout.line!(Str.concat("large: ", u64_str(large.n))) ?? {}

	# Redirect chain: followed to /final by default.
	match TempTest.send!(get_req(Str.concat(base, "/redirect"), 5000)) {
		Ok(resp) => {
			d = drain!(resp.body_stream, 0, False)
			Stdout.line!(Str.concat("redirect: ", Str.concat(u16_str(resp.status), Str.concat(" ", u64_str(d.n))))) ?? {}
		}
		Err(_) => Stdout.line!("redirect: failed") ?? {}
	}

	# Multi-value header: both X-Multi occurrences survive (NUL-joined -> '|').
	match TempTest.send!(get_req(Str.concat(base, "/multi"), 5000)) {
		Ok(resp) => {
			flat = flat_str(resp.headers_flat)
			_ = drain!(resp.body_stream, 0, False)
			Stdout.line!(Str.concat("multi: ", flat)) ?? {}
		}
		Err(_) => Stdout.line!("multi: failed") ?? {}
	}

	# Decode coverage (H2): chunked is de-chunked; gzip/brotli decompress
	# transparently — the stream yields the ORIGINAL text either way.
	Stdout.line!(Str.concat("chunked: ", get_text!(Str.concat(base, "/chunked")))) ?? {}
	Stdout.line!(Str.concat("gzip: ", get_text!(Str.concat(base, "/gzip")))) ?? {}
	Stdout.line!(Str.concat("brotli: ", get_text!(Str.concat(base, "/brotli")))) ?? {}

	# Mid-body cutoff: send! succeeds, a read partway through fails (StreamErr).
	match TempTest.send!(get_req(Str.concat(base, "/truncate"), 5000)) {
		Ok(resp) => {
			d = drain!(resp.body_stream, 0, False)
			Stdout.line!(Str.concat("truncate: ", Str.concat(u64_str(d.n), if d.errored { " errored" } else { " clean" }))) ?? {}
		}
		Err(_) => Stdout.line!("truncate: send-failed") ?? {}
	}

	# Stall: no response within the timeout -> send! reports Timeout.
	stall = match TempTest.send!(get_req(Str.concat(base, "/stall"), 300)) {
		Ok(_) => "unexpected-ok"
		Err(Timeout) => "timeout"
		Err(NetworkError) => "network"
		Err(BadBody) => "badbody"
		Err(Other(_)) => "other"
	}
	Stdout.line!(Str.concat("stall: ", stall)) ?? {}

	Err(Exit(Sockets.live!({})))
}

## Build a GET request record for the primitive (method 3 = GET, no headers/body).
get_req : Str, U64 -> { method : U8, method_ext : Str, headers : List((Str, Str)), uri : Str, body : List(U8), timeout_ms : U64 }
get_req = |uri, timeout_ms| { method: 3, method_ext: "", headers: [], uri, body: [], timeout_ms }

## Read a body stream to end. Threads the stream so Roc re-incs it before each
## `read!` (owned per call); drops it at a base case. Returns bytes read and
## whether a read errored mid-stream (a truncated/reset body, H15).
drain! : Streams.InputStream, U64, Bool => { n : U64, errored : Bool }
drain! = |stream, acc, _err| {
	match Streams.read!(stream, 4096) {
		Ok(chunk) => if List.is_empty(chunk) { { n: acc, errored: False } } else { drain!(stream, acc + List.len(chunk), False) }
		Err(_) => { n: acc, errored: True }
	}
}

send_and_drain! : Str, U64 => { n : U64, errored : Bool }
send_and_drain! = |uri, timeout_ms| {
	match TempTest.send!(get_req(uri, timeout_ms)) {
		Ok(resp) => drain!(resp.body_stream, 0, False)
		Err(_) => { n: 0, errored: True }
	}
}

## GET a URL and return its (decoded) body as text.
get_text! : Str => Str
get_text! = |uri| {
	match TempTest.send!(get_req(uri, 5000)) {
		Ok(resp) => Str.from_utf8_lossy(collect_bytes!(resp.body_stream, []))
		Err(_) => "err"
	}
}

collect_bytes! : Streams.InputStream, List(U8) => List(U8)
collect_bytes! = |stream, acc| {
	match Streams.read!(stream, 65536) {
		Ok(chunk) => if List.is_empty(chunk) { acc } else { collect_bytes!(stream, List.concat(acc, chunk)) }
		Err(_) => acc
	}
}

## NUL-joined header bytes -> a printable string (NUL shown as '|').
flat_str : List(U8) -> Str
flat_str = |bytes| Str.from_utf8_lossy(List.map(bytes, |b| if b == 0 { 124 } else { b }))

u16_str : U16 -> Str
u16_str = |n| if n == 0 { "0" } else { go16(n, "") }
go16 : U16, Str -> Str
go16 = |n, acc| if n == 0 { acc } else { go16(n // 10, Str.concat(digit(n % 10), acc)) }

u64_str : U64 -> Str
u64_str = |n| if n == 0 { "0" } else { go64(n, "") }
go64 : U64, Str -> Str
go64 = |n, acc| if n == 0 { acc } else { go64(n // 10, Str.concat(digit64(n % 10), acc)) }

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
digit64 : U64 -> Str
digit64 = |d| {
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
