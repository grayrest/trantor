//! `local-cert <cert-path>` — write a self-signed localhost cert to <cert-path>
//! and its key to <cert-path>.key, then print the `export TRANTOR_HTTP_EXTRA_CA`
//! line to wire it into trantor's HTTP client (H13/H14).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cert_path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: local-cert <cert-path>   (key written to <cert-path>.key)");
            return ExitCode::FAILURE;
        }
    };
    let key_path = format!("{cert_path}.key");

    let pem = match local_cert::generate() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("local-cert: generate failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(&cert_path, &pem.cert) {
        eprintln!("local-cert: write {cert_path}: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&key_path, &pem.key) {
        eprintln!("local-cert: write {key_path}: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!("local-cert: wrote {cert_path} (cert) and {key_path} (key), SANs localhost/127.0.0.1/::1");
    println!("export TRANTOR_HTTP_EXTRA_CA={cert_path}");
    ExitCode::SUCCESS
}
