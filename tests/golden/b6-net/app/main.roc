app [main!] { pf: platform "../platform/main.roc" }
import pf.Stdout
import pf.Tcp
import pf.Http
import pf.Udp
import pf.Sockets
import pf.Streams
import pf.TestNet
import pf.Url

## basic-cli-shaped networking: Tcp (verbatim), Http (verbatim), plus Udp and a
## listen/accept via the sockets primitives. Peers are TestNet helper threads.
## Exit code == live socket/stream resources at the end (0 = drop-balanced).
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	echo_port = TestNet.start_tcp_echo!({})
	tcp_line = match Tcp.connect!("127.0.0.1", echo_port, 2000) {
		Ok(s) => {
			Tcp.Stream.write_utf8!(s, "hi", 2000) ?? {}
			match Tcp.Stream.read_up_to!(s, 64, 2000) {
				Ok(b) => Str.from_utf8_lossy(b)
				Err(_) => "read-err"
			}
		}
		Err(_) => "connect-err"
	}
	Stdout.line!(Str.concat("tcp-echo: ", tcp_line)) ?? {}

	http_port = TestNet.start_httpd!({})
	body = match Url.parse(Str.concat("http://127.0.0.1:", Str.concat(u16_str(http_port), "/"))) {
		Ok(url) => {
			match Http.get_utf8!(url) {
				Ok(t) => t
				Err(_) => "http-failed"
			}
		}
		Err(_) => "bad-url"
	}
	Stdout.line!(Str.concat("http-get: ", body)) ?? {}

	udp_port = TestNet.start_udp_echo!({})
	udp_line = match Udp.bind!("127.0.0.1", 0) {
		Ok(u) => {
			_ = Udp.send_to!(u, "127.0.0.1", udp_port, Str.to_utf8("dgram")) ?? 0
			match Udp.recv!(u, 64) {
				Ok(r) => Str.from_utf8_lossy(r.bytes)
				Err(_) => "recv-failed"
			}
		}
		Err(_) => "bind-failed"
	}
	Stdout.line!(Str.concat("udp-echo: ", udp_line)) ?? {}

	accepted = match Sockets.tcp_listen!("127.0.0.1", 0) {
		Ok(listener) => {
			TestNet.connect_and_send_later!(Sockets.tcp_local_port!(listener), 50)
			match Sockets.tcp_accept!(listener) {
				Ok(conn) => Str.from_utf8_lossy(Streams.read!(Sockets.tcp_input!(conn), 64) ?? [])
				Err(_) => "accept-failed"
			}
		}
		Err(_) => "listen-failed"
	}
	Stdout.write!(Str.concat("tcp-accept: ", accepted)) ?? {}
	Err(Exit(Sockets.live!({})))
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
