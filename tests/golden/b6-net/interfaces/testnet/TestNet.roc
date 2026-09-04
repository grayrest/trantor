## TEST SCAFFOLDING (B6 only): each starts a thread and returns its port.
TestNet :: [].{
	start_tcp_echo! : {} => U16
	start_udp_echo! : {} => U16
	start_httpd! : {} => U16
	## After `ms`, a thread connects to 127.0.0.1:port and sends "ping\n".
	connect_and_send_later! : U16, U64 => {}
}
