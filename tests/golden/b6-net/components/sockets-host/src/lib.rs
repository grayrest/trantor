//! roc:sync-sockets host: tcp (client+listen), udp, name lookup. Sockets are
//! resources (P5); bytes flow through sync-io streams minted per call from a
//! cloned socket. Blocking (P12). Owned args decref'd; handles via resource::with.
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::*;
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};

pub enum Sock { Stream(TcpStream), Listener(TcpListener), Udp(UdpSocket) }

fn ioerr(e: &std::io::Error) -> IOErr {
    use std::io::ErrorKind as K;
    let tag = match e.kind() { K::NotFound => IOErrTag::NotFound, K::PermissionDenied => IOErrTag::PermissionDenied, K::AlreadyExists => IOErrTag::AlreadyExists, K::BrokenPipe => IOErrTag::BrokenPipe, K::Interrupted => IOErrTag::Interrupted, _ => IOErrTag::Other };
    if let IOErrTag::Other = tag { IOErr { payload: IOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag } } else { IOErr { payload: unsafe { core::mem::zeroed() }, tag } }
}
fn take_str(s: RocStr) -> String { let v = s.as_str().to_string(); unsafe { s.decref(abi::host()) }; v }
fn handle(s: Sock) -> *mut u64 { abi::resource::new(s) as *mut u64 }

macro_rules! sock_result { ($R:ident, $P:ident, $T:ident, $r:expr) => {
    match $r { Ok(s) => $R { payload: $P { ok: ManuallyDrop::new(handle(s)) }, tag: $T::Ok }, Err(e) => $R { payload: $P { err: ManuallyDrop::new(ioerr(&e)) }, tag: $T::Err } }
}}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__resolve(name: RocStr) -> SocketsResolveResult {
    let host = take_str(name);
    match format!("{host}:0").to_socket_addrs() {
        Ok(addrs) => {
            let v: Vec<V4OrV6> = addrs.map(|a| match a.ip() {
                std::net::IpAddr::V4(ip) => { let o = ip.octets(); V4OrV6 { payload: V4OrV6Payload { v4: ManuallyDrop::new(V4OrV6V4Payload { _0: o[0], _1: o[1], _2: o[2], _3: o[3] }) }, tag: V4OrV6Tag::V4 } }
                std::net::IpAddr::V6(ip) => { let s = ip.segments(); V4OrV6 { payload: V4OrV6Payload { v6: ManuallyDrop::new(V4OrV6V6Payload { _0: s[0], _1: s[1], _2: s[2], _3: s[3], _4: s[4], _5: s[5], _6: s[6], _7: s[7] }) }, tag: V4OrV6Tag::V6 } }
            }).collect();
            SocketsResolveResult { payload: SocketsResolveResultPayload { ok: ManuallyDrop::new(unsafe { RocListWith::<V4OrV6, false>::from_slice(&v, abi::host()) }) }, tag: SocketsResolveResultTag::Ok }
        }
        Err(e) => { let t = ioerr(&e); SocketsResolveResult { payload: SocketsResolveResultPayload { err: ManuallyDrop::new(unsafe { core::mem::transmute::<IOErr, SocketsIOErr>(t) }) }, tag: SocketsResolveResultTag::Err } }
    }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_connect(host: RocStr, port: u16) -> SocketsTcpConnectResult {
    let h = take_str(host);
    sock_result!(SocketsTcpConnectResult, SocketsTcpConnectResultPayload, SocketsTcpConnectResultTag, TcpStream::connect((h.as_str(), port)).map(Sock::Stream))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_listen(host: RocStr, port: u16) -> SocketsTcpListenResult {
    let h = take_str(host);
    sock_result!(SocketsTcpListenResult, SocketsTcpListenResultPayload, SocketsTcpListenResultTag, TcpListener::bind((h.as_str(), port)).map(Sock::Listener))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_accept(l: *mut u64) -> SocketsTcpAcceptResult {
    let r = unsafe { abi::resource::with(l as RocBox, |s: &mut Sock| match s { Sock::Listener(l) => l.accept().map(|(c, _)| Sock::Stream(c)), _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "not a listener")) }) };
    sock_result!(SocketsTcpAcceptResult, SocketsTcpAcceptResultPayload, SocketsTcpAcceptResultTag, r)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_input(s: *mut u64) -> *mut u64 {
    let r: Box<dyn std::io::Read> = unsafe { abi::resource::with(s as RocBox, |x: &mut Sock| match x { Sock::Stream(t) => t.try_clone().map(|c| Box::new(c) as Box<dyn std::io::Read>).unwrap_or_else(|_| Box::new(std::io::empty())), _ => Box::new(std::io::empty()) }) };
    sync_io_core::input_stream(r) as *mut u64
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_output(s: *mut u64) -> *mut u64 {
    let w: Box<dyn std::io::Write> = unsafe { abi::resource::with(s as RocBox, |x: &mut Sock| match x { Sock::Stream(t) => t.try_clone().map(|c| Box::new(c) as Box<dyn std::io::Write>).unwrap_or_else(|_| Box::new(std::io::sink())), _ => Box::new(std::io::sink()) }) };
    sync_io_core::output_stream(w) as *mut u64
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_set_read_timeout(s: *mut u64, ms: u64) {
    unsafe { abi::resource::with(s as RocBox, |x: &mut Sock| { if let Sock::Stream(t) = x { let _ = t.set_read_timeout(if ms == 0 { None } else { Some(std::time::Duration::from_millis(ms)) }); } }) }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__tcp_local_port(s: *mut u64) -> u16 {
    unsafe { abi::resource::with(s as RocBox, |x: &mut Sock| match x { Sock::Stream(t) => t.local_addr().map(|a| a.port()).unwrap_or(0), Sock::Listener(l) => l.local_addr().map(|a| a.port()).unwrap_or(0), Sock::Udp(u) => u.local_addr().map(|a| a.port()).unwrap_or(0) }) }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__udp_bind(host: RocStr, port: u16) -> SocketsUdpBindResult {
    let h = take_str(host);
    sock_result!(SocketsUdpBindResult, SocketsUdpBindResultPayload, SocketsUdpBindResultTag, UdpSocket::bind((h.as_str(), port)).map(Sock::Udp))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__udp_send_to(s: *mut u64, host: RocStr, port: u16, bytes: RocListWith<u8, false>) -> SocketsUdpSendToResult {
    let h = take_str(host); let b = bytes.as_slice().to_vec(); unsafe { bytes.decref(abi::host()) };
    let r = unsafe { abi::resource::with(s as RocBox, |x: &mut Sock| match x { Sock::Udp(u) => u.send_to(&b, (h.as_str(), port)), _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "not udp")) }) };
    match r { Ok(n) => SocketsUdpSendToResult { payload: SocketsUdpSendToResultPayload { ok: ManuallyDrop::new(n as u64) }, tag: SocketsUdpSendToResultTag::Ok }, Err(e) => SocketsUdpSendToResult { payload: SocketsUdpSendToResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: SocketsUdpSendToResultTag::Err } }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__udp_recv(s: *mut u64, max: u64) -> SocketsUdpRecvResult {
    let r = unsafe { abi::resource::with(s as RocBox, |x: &mut Sock| match x { Sock::Udp(u) => { let mut buf = vec![0u8; max as usize]; u.recv_from(&mut buf).map(|(n, from)| { buf.truncate(n); (buf, from) }) }, _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "not udp")) }) };
    match r {
        Ok((buf, from)) => SocketsUdpRecvResult { payload: SocketsUdpRecvResultPayload { ok: ManuallyDrop::new(AnonStruct902edcae59c36540 { bytes: unsafe { RocListWith::<u8, false>::from_slice(&buf, abi::host()) }, from_host: RocStr::from_str(&from.ip().to_string(), abi::host()), from_port: from.port() }) }, tag: SocketsUdpRecvResultTag::Ok },
        Err(e) => SocketsUdpRecvResult { payload: SocketsUdpRecvResultPayload { err: ManuallyDrop::new(ioerr(&e)) }, tag: SocketsUdpRecvResultTag::Err },
    }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__udp_local_port(s: *mut u64) -> u16 { trantor__sockets_host__tcp_local_port(s) }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__sockets_host__live() -> i32 { abi::resource::live() as i32 }
