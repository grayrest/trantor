import IOErr exposing [IOErr]
import Sockets
## Udp: thin blocking sugar over roc:sync-sockets/udp (no basic-cli precedent).
Udp :: [].{
	Socket : Sockets.UdpSocket
	bind! : Str, U16 => Try(Socket, [BindErr(IOErr)])
	bind! = |host, port| Sockets.udp_bind!(host, port)
	local_port! : Socket => U16
	local_port! = |s| Sockets.udp_local_port!(s)
	send_to! : Socket, Str, U16, List(U8) => Try(U64, [SendErr(IOErr)])
	send_to! = |s, host, port, bytes| Sockets.udp_send_to!(s, host, port, bytes)
	recv! : Socket, U64 => Try({ bytes : List(U8), from_host : Str, from_port : U16 }, [RecvErr(IOErr)])
	recv! = |s, max| Sockets.udp_recv!(s, max)
}
