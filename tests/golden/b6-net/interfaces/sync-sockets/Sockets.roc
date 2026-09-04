import IOErr exposing [IOErr]
import Streams
## roc:sync-sockets: WASI's socket surface in blocking form (P6/P7/P12).
## Sockets are resources (P5); bytes flow through sync-io streams minted per
## call from a socket. Addresses are host:port pairs (name lookup is `resolve!`).
Sockets :: [].{
	TcpSocket :: Box(U64)
	UdpSocket :: Box(U64)
	IpAddress : [V4(U8, U8, U8, U8), V6(U16, U16, U16, U16, U16, U16, U16, U16)]
	## ip-name-lookup: every address a name resolves to.
	resolve! : Str => Try(List(IpAddress), [LookupErr(IOErr)])
	## tcp-create-socket + connect, in one blocking step.
	tcp_connect! : Str, U16 => Try(TcpSocket, [ConnectErr(IOErr)])
	## tcp-create-socket + bind + listen; port 0 picks a free port.
	tcp_listen! : Str, U16 => Try(TcpSocket, [ListenErr(IOErr)])
	tcp_accept! : TcpSocket => Try(TcpSocket, [AcceptErr(IOErr)])
	tcp_input! : TcpSocket => Streams.InputStream
	tcp_output! : TcpSocket => Streams.OutputStream
	## Blocking-model stand-in for pollable timeouts: 0 = none.
	tcp_set_read_timeout! : TcpSocket, U64 => {}
	tcp_local_port! : TcpSocket => U16
	udp_bind! : Str, U16 => Try(UdpSocket, [BindErr(IOErr)])
	udp_send_to! : UdpSocket, Str, U16, List(U8) => Try(U64, [SendErr(IOErr)])
	## One datagram (up to `max` bytes) and its sender.
	udp_recv! : UdpSocket, U64 => Try({ bytes : List(U8), from_host : Str, from_port : U16 }, [RecvErr(IOErr)])
	udp_local_port! : UdpSocket => U16
	live! : {} => I32
}
