import TestNet
import HttpHost

## TEMP TESTING namespace — NOT part of the platform's real surface. It bundles
## the throwaway test scaffolding under one clearly-named module so it doesn't
## sit in `exposes` masquerading as API: the testnet peer threads, and the raw
## `send!` primitive the client-driving tests exercise directly (apps normally
## use the derived `Http`). Forwards to the internal `testnet` + `sync-http`
## interfaces. To be replaced when a real http serving API is built.
TempTest :: [].{
	start_tcp_echo! : {} => U16
	start_tcp_echo! = |_| TestNet.start_tcp_echo!({})

	start_udp_echo! : {} => U16
	start_udp_echo! = |_| TestNet.start_udp_echo!({})

	start_httpd! : {} => U16
	start_httpd! = |_| TestNet.start_httpd!({})

	connect_and_send_later! : U16, U64 => {}
	connect_and_send_later! = |port, ms| TestNet.connect_and_send_later!(port, ms)

	start_https_server! : {} => U16
	start_https_server! = |_| TestNet.start_https_server!({})

	## The raw HTTP client primitive (normally reached via the derived `Http`).
	send! : HttpHost.Request => Try(HttpHost.Response, HttpHost.TransportErr)
	send! = |req| HttpHost.send!(req)
}
