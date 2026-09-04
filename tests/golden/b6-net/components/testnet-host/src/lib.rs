//! TEST SCAFFOLDING (B6 only): peer threads so a blocking single-threaded app
//! can act as a client. Each binds 127.0.0.1:0 and returns its port.
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
    std::thread::spawn(move || { for c in l.incoming().flatten() { std::thread::spawn(move || {
        let mut c = c; let mut req = Vec::new(); let mut b = [0u8; 512];
        while let Ok(n) = c.read(&mut b) { if n == 0 { break; } req.extend_from_slice(&b[..n]); if req.windows(4).any(|w| w == b"\r\n\r\n") { break; } }
        // Only a GET gets the expected body: makes the host's method encoding observable.
        let body: String = if req.starts_with(b"GET ") { "hello-http".into() } else { format!("bad-verb:{}", String::from_utf8_lossy(&req).split(' ').next().unwrap_or("")) };
        let _ = c.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
    }); } });
    port
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__testnet_host__connect_and_send_later(port: u16, ms: u64) {
    std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(ms)); if let Ok(mut c) = TcpStream::connect(("127.0.0.1", port)) { let _ = c.write_all(b"ping\n"); } });
}
