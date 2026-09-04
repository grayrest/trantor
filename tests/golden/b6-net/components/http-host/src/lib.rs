//! roc:sync-http host over ureq (H1): one blocking outgoing request whose
//! response BODY is a streaming roc:sync-io InputStream (H5). One process-wide
//! Agent (H8) gives keep-alive + pooling; chunked decode is built in and
//! gzip/brotli decompress transparently (H2). `send!` returns status +
//! NUL-joined multi-value headers (H10) as soon as the final headers arrive;
//! the body is read lazily through the InputStream. Method u8 is basic-cli's
//! InternalHttp.to_host_method encoding (CONNECT=0 … TRACE=9), `Unknown` told
//! apart by a non-empty method_ext (H15).
use core::mem::ManuallyDrop;
use hematite_abi as abi;
use abi::*;
use std::sync::OnceLock;
use std::time::Duration;
use ureq::{http, Agent};

const METHODS: [&str; 10] = ["CONNECT", "DELETE", "QUERY", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT", "TRACE"];
type ErrTag = BadBodyOrNetworkErrorOrOtherOrTimeoutTag;
type Err = BadBodyOrNetworkErrorOrOtherOrTimeout;
type Ok = AnonStructD04f4a420a7c0c28;

/// The one shared Agent (H8): built once, reused for every send! so connections
/// pool and (in HC4) TLS initializes once. Redirect policy from the env (H16):
/// HEMATITE_HTTP_MAX_REDIRECTS (default 10), and hitting the ceiling returns the
/// last response rather than erroring (browser-like). Non-2xx is success (H3).
fn agent() -> &'static Agent {
    static A: OnceLock<Agent> = OnceLock::new();
    A.get_or_init(|| {
        let max_redirects: u32 = std::env::var("HEMATITE_HTTP_MAX_REDIRECTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(max_redirects)
            .max_redirects_will_error(false)
            .build();
        Agent::new_with_config(config)
    })
}

fn err(tag: ErrTag, msg: &str) -> HttpHostSendResult {
    let e = if let ErrTag::Other = tag {
        Err {
            payload: BadBodyOrNetworkErrorOrOtherOrTimeoutPayload {
                other: ManuallyDrop::new(unsafe { RocListWith::<u8, false>::from_slice(msg.as_bytes(), abi::host()) }),
            },
            tag,
        }
    } else {
        Err { payload: unsafe { core::mem::zeroed() }, tag }
    };
    HttpHostSendResult { payload: HttpHostSendResultPayload { err: ManuallyDrop::new(e) }, tag: HttpHostSendResultTag::Err }
}

/// Map a ureq transport error onto the 4-variant twin (H3). `send!` only reports
/// connect/header-phase failures; body-phase failures surface as StreamErr on a
/// later read (H15), so decompression/truncation are not mapped here.
fn send_err(e: &ureq::Error) -> HttpHostSendResult {
    use ureq::Error as E;
    match e {
        E::Timeout(_) => err(ErrTag::Timeout, ""),
        E::Io(_) | E::HostNotFound | E::ConnectionFailed | E::Tls(_) | E::ConnectProxyFailed(_) | E::TlsRequired | E::RequireHttpsOnly(_) => {
            err(ErrTag::NetworkError, "")
        }
        E::Protocol(_) | E::BodyExceedsLimit(_) | E::LargeResponseHeader(_, _) => err(ErrTag::BadBody, ""),
        other => err(ErrTag::Other, &other.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__http_host__send(a: HttpHostSendArgs) -> HttpHostSendResult {
    let uri = a.uri.as_str().to_string();
    let method_str = if !a.method_ext.is_empty() {
        a.method_ext.as_str().to_string()
    } else {
        METHODS.get(a.method as usize).copied().unwrap_or("GET").to_string()
    };
    let headers: Vec<(String, String)> = a.headers.as_slice().iter().map(|h| (h._0.as_str().to_string(), h._1.as_str().to_string())).collect();
    let body = a.body.as_slice().to_vec();
    let timeout = a.timeout_ms;
    unsafe { a.decref(abi::host()); } // whole-struct decref recurses into header element strings (B0)

    let method = match http::Method::from_bytes(method_str.as_bytes()) {
        Ok(m) => m,
        Err(_) => return err(ErrTag::Other, "invalid HTTP method"),
    };
    let mut builder = http::Request::builder().method(method).uri(&uri);
    for (k, v) in &headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    let request = match builder.body(body) {
        Ok(r) => r,
        Err(e) => return err(ErrTag::Other, &format!("bad request: {e}")),
    };

    let ag = agent();
    // Per-request timeout (H9, revised): with no between-bytes timeout in ureq
    // 3.4, timeout_ms bounds connect + response-headers AND the total body
    // receive (recv_body) — a stall trips Timeout; a very large/slow body is
    // also bounded (basic-cli-like overall deadline for the body). 0 = none.
    let configured = if timeout > 0 {
        let d = Duration::from_millis(timeout);
        ag.configure_request(request)
            .timeout_connect(Some(d))
            .timeout_recv_response(Some(d))
            .timeout_recv_body(Some(d))
            .build()
    } else {
        request
    };

    let resp = match ag.run(configured) {
        Ok(r) => r,
        Err(e) => return send_err(&e),
    };

    let status = resp.status().as_u16();
    // NUL-joined, every occurrence preserved and in order (H10): correct for
    // repeated Set-Cookie; the shim splits back into List (Str, Str).
    let mut flat = Vec::new();
    for (name, value) in resp.headers().iter() {
        if !flat.is_empty() {
            flat.push(0);
        }
        flat.extend_from_slice(name.as_str().as_bytes());
        flat.push(0);
        flat.extend_from_slice(value.as_bytes());
    }
    // The body joins files/sockets/stdin as an InputStream (H5): mint one from
    // ureq's owned 'static body reader (decoded: chunked undone, gzip/brotli
    // decompressed). Early-dropping the stream recycles the connection via the
    // B0 destructor.
    let reader = resp.into_body().into_reader();
    let body_stream = sync_io_core::input_stream(Box::new(reader)) as *mut u64;

    let ok = Ok { body_stream, headers_flat: unsafe { RocListWith::<u8, false>::from_slice(&flat, abi::host()) }, status };
    HttpHostSendResult { payload: HttpHostSendResultPayload { ok: ManuallyDrop::new(ok) }, tag: HttpHostSendResultTag::Ok }
}
