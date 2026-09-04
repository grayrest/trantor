//! TEST SCAFFOLDING (B6/HC2 only): peer threads so a blocking single-threaded
//! app can act as a client. Each binds 127.0.0.1:0 and returns its port. The
//! httpd routes by path so HC2 can exercise a large streamed body, a redirect
//! chain, multi-value headers, a stall (send-phase timeout), a mid-body cutoff
//! (StreamErr), and chunked transfer-encoding — all on one port.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__testnet_host__start_tcp_echo() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind"); let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || { for c in l.incoming().flatten() { std::thread::spawn(move || { let mut c = c; let mut b = [0u8; 1024]; while let Ok(n) = c.read(&mut b) { if n == 0 || c.write_all(&b[..n]).is_err() { break; } } }); } });
    port
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__testnet_host__start_udp_echo() -> u16 {
    let u = UdpSocket::bind("127.0.0.1:0").expect("bind"); let port = u.local_addr().unwrap().port();
    std::thread::spawn(move || { let mut b = [0u8; 1024]; while let Ok((n, from)) = u.recv_from(&mut b) { let _ = u.send_to(&b[..n], from); } });
    port
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__testnet_host__start_httpd() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind"); let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || { for c in l.incoming().flatten() { std::thread::spawn(move || serve(c)); } });
    port
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__testnet_host__connect_and_send_later(port: u16, ms: u64) {
    std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(ms)); if let Ok(mut c) = TcpStream::connect(("127.0.0.1", port)) { let _ = c.write_all(b"ping\n"); } });
}

fn headers(code: u16, reason: &str, len: usize) -> String {
    format!("HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n")
}

fn serve(mut c: TcpStream) {
    let mut req = Vec::new(); let mut b = [0u8; 512];
    loop {
        match c.read(&mut b) { Ok(0) | Err(_) => return, Ok(n) => { req.extend_from_slice(&b[..n]); if req.windows(4).any(|w| w == b"\r\n\r\n") { break; } } }
    }
    let head = String::from_utf8_lossy(&req);
    let mut parts = head.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    match path {
        // B6: only a GET gets the body, making the method encoding observable.
        "/" => {
            let body: String = if method == "GET" { "hello-http".into() } else { format!("bad-verb:{method}") };
            let _ = c.write_all(headers(200, "OK", body.len()).as_bytes());
            let _ = c.write_all(body.as_bytes());
        }
        // HC2: a large body, streamed in chunks (Content-Length known).
        "/large" => {
            let n = 100_000usize;
            let _ = c.write_all(headers(200, "OK", n).as_bytes());
            let chunk = vec![b'x'; 8192];
            let mut sent = 0;
            while sent < n { let take = (n - sent).min(chunk.len()); if c.write_all(&chunk[..take]).is_err() { break; } sent += take; }
        }
        // HC2: a redirect chain -> /final (ureq follows unless max_redirects=0).
        "/redirect" => { let _ = c.write_all(b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); }
        "/final" => { let body = "final-body"; let _ = c.write_all(headers(200, "OK", body.len()).as_bytes()); let _ = c.write_all(body.as_bytes()); }
        // HC2: the same header name twice (H10 — every occurrence preserved).
        "/multi" => { let body = "multi"; let _ = c.write_all(format!("HTTP/1.1 200 OK\r\nX-Multi: a\r\nX-Multi: b\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()); }
        // HC2: accept, read the request, then hang WITHOUT responding -> the
        // client's send! trips its recv_response timeout (Timeout).
        "/stall" => { std::thread::sleep(std::time::Duration::from_secs(5)); }
        // HC2: promise 100 bytes, send 10, then close -> a mid-body read fails
        // (StreamErr, H15). send! itself succeeds (headers arrived).
        "/truncate" => { let _ = c.write_all(headers(200, "OK", 100).as_bytes()); let _ = c.write_all(&[b'y'; 10]); }
        // HC2/HC5: chunked transfer-encoding; ureq decodes to "hello-world".
        "/chunked" => { let _ = c.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n-world\r\n0\r\n\r\n"); }
        _ => { let _ = c.write_all(headers(404, "Not Found", 0).as_bytes()); }
    }
}
