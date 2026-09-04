//! roc:sync-http host: one blocking HTTP/1.1 request over std::net (no crate
//! dep). Method u8 is basic-cli's InternalHttp.to_host_method encoding
//! (CONNECT=0 … TRACE=9); `Unknown` shares QUERY's 2 and is told apart by a
//! non-empty method_ext. Response headers return NUL-joined (R-B5; shim splits).
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::*;
use std::io::{Read, Write};

const METHODS: [&str; 10] = ["CONNECT", "DELETE", "QUERY", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT", "TRACE"];
type Err = BadBodyOrNetworkErrorOrOtherOrTimeout;
fn err(tag: BadBodyOrNetworkErrorOrOtherOrTimeoutTag, msg: &str) -> HttpHostSendResult {
    let e = if let BadBodyOrNetworkErrorOrOtherOrTimeoutTag::Other = tag {
        Err { payload: BadBodyOrNetworkErrorOrOtherOrTimeoutPayload { other: ManuallyDrop::new(unsafe { RocListWith::<u8, false>::from_slice(msg.as_bytes(), abi::host()) }) }, tag }
    } else { Err { payload: unsafe { core::mem::zeroed() }, tag } };
    HttpHostSendResult { payload: HttpHostSendResultPayload { err: ManuallyDrop::new(e) }, tag: HttpHostSendResultTag::Err }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__http_host__send(a: HttpHostSendArgs) -> HttpHostSendResult {
    use BadBodyOrNetworkErrorOrOtherOrTimeoutTag as T;
    let uri = a.uri.as_str().to_string();
    let method = if !a.method_ext.is_empty() { a.method_ext.as_str().to_string() } else { METHODS.get(a.method as usize).copied().unwrap_or("GET").to_string() };
    let headers: Vec<(String, String)> = a.headers.as_slice().iter().map(|h| (h._0.as_str().to_string(), h._1.as_str().to_string())).collect();
    let body = a.body.as_slice().to_vec();
    let timeout = a.timeout_ms;
    unsafe { a.decref(abi::host()); } // whole-struct decref recurses into headers element strings (B0)

    let rest = match uri.strip_prefix("http://") { Some(r) => r, None => return err(T::Other, "only http:// is supported") };
    let (hostport, path) = match rest.find('/') { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "/") };
    let (host, port) = match hostport.rsplit_once(':') { Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)), None => (hostport, 80) };
    let mut s = match std::net::TcpStream::connect((host, port)) { Ok(s) => s, Err(_) => return err(T::NetworkError, "") };
    if timeout > 0 { let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(timeout))); }
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
    for (k, v) in &headers { req.push_str(&format!("{k}: {v}\r\n")); }
    req.push_str("\r\n");
    if s.write_all(req.as_bytes()).and_then(|_| s.write_all(&body)).is_err() { return err(T::NetworkError, ""); }
    let mut raw = Vec::new();
    if let Err(e) = s.read_to_end(&mut raw) { return err(if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut { T::Timeout } else { T::NetworkError }, ""); }
    let split = match raw.windows(4).position(|w| w == b"\r\n\r\n") { Some(i) => i, None => return err(T::BadBody, "") };
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body_out = raw[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next().and_then(|l| l.split(' ').nth(1)).and_then(|c| c.parse().ok()).unwrap_or(0);
    let mut flat = Vec::new();
    for l in lines { if let Some((k, v)) = l.split_once(':') { if !flat.is_empty() { flat.push(0); } flat.extend_from_slice(k.trim().as_bytes()); flat.push(0); flat.extend_from_slice(v.trim().as_bytes()); } }
    HttpHostSendResult { payload: HttpHostSendResultPayload { ok: ManuallyDrop::new(AnonStruct56fc9c151eeedf9 {
        body: unsafe { RocListWith::<u8, false>::from_slice(&body_out, abi::host()) },
        headers_flat: unsafe { RocListWith::<u8, false>::from_slice(&flat, abi::host()) },
        status }) }, tag: HttpHostSendResultTag::Ok }
}
