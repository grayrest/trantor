//! Generate a self-signed certificate for local TLS testing/development
//! (HC1/H14). SANs cover `localhost`/`127.0.0.1`/`::1`; validity is long so a
//! committed dev cert (if anyone keeps one) does not silently expire. Both the
//! `just make-local-cert` recipe and hematite's HTTP test suite mint certs
//! through `generate()` — the suite makes an ephemeral one per run, so no
//! private key is ever committed.

use rcgen::{CertificateParams, DnType, KeyPair, KeyUsagePurpose, ExtendedKeyUsagePurpose, date_time_ymd};

/// A freshly generated cert + key, both PEM-encoded.
pub struct Pem {
    pub cert: String,
    pub key: String,
}

/// Generate a self-signed localhost server certificate. Deterministic in shape,
/// random in key material (a new key each call).
pub fn generate() -> Result<Pem, rcgen::Error> {
    // "127.0.0.1"/"::1" are parsed as IP SANs, "localhost" as a DNS SAN.
    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])?;
    // Long validity: an ephemeral test cert never expires mid-suite, and a dev
    // cert someone parks under HEMATITE_HTTP_EXTRA_CA keeps working for years.
    params.not_before = date_time_ymd(2020, 1, 1);
    params.not_after = date_time_ymd(2100, 1, 1);
    params.distinguished_name.push(DnType::CommonName, "hematite local-cert");
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    Ok(Pem { cert: cert.pem(), key: key.serialize_pem() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_cert_loads_into_a_rustls_root_store() {
        let pem = generate().expect("generate");
        // Parse the cert PEM the way a trust store would, and add it as a
        // trust anchor — proves rustls accepts what we emit (HC1 exit).
        let mut rd = std::io::BufReader::new(pem.cert.as_bytes());
        let certs: Vec<_> = rustls_pemfile::certs(&mut rd).collect::<Result<_, _>>().expect("parse cert PEM");
        assert_eq!(certs.len(), 1, "expected exactly one certificate in the PEM");
        let mut store = rustls::RootCertStore::empty();
        store.add(certs[0].clone()).expect("rustls should accept the cert as a trust anchor");
        assert_eq!(store.len(), 1);

        // The key PEM must parse as a PKCS#8 private key.
        let mut kr = std::io::BufReader::new(pem.key.as_bytes());
        let key = rustls_pemfile::private_key(&mut kr).expect("parse key PEM");
        assert!(key.is_some(), "expected a private key in the key PEM");
    }
}
