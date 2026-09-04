#!/usr/bin/env bash
# HC1 acceptance: the local-cert crate + `just make-local-cert` (H14).
#   1. `cargo test` runs the crate's unit test: the generated cert PEM loads
#      into a rustls RootCertStore (and the key PEM parses as PKCS#8).
#   2. `just make-local-cert <tmp>` writes a cert+key that openssl parses, with
#      the localhost SAN, and prints the export line.
# Standalone, no HTTP — the deterministic TLS handshake proof is HC4.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root
MANIFEST=tools/local-cert/Cargo.toml

# ---- 1. unit test: PEM -> rustls RootCertStore ----
cargo test --quiet --manifest-path "$MANIFEST" >/dev/null 2>&1 || { echo "FAIL: cargo test -p local-cert"; cargo test --manifest-path "$MANIFEST"; exit 1; }
echo "ok: generated cert PEM loads into a rustls RootCertStore (unit test)"

# ---- 2. just make-local-cert produces parseable files ----
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
CERT="$TMP/dev-cert.pem"
out=$(just make-local-cert "$CERT" 2>/dev/null)
[[ -f "$CERT" ]] || { echo "FAIL: cert not written to $CERT"; exit 1; }
[[ -f "$CERT.key" ]] || { echo "FAIL: key not written to $CERT.key"; exit 1; }
[[ "$out" == "export HEMATITE_HTTP_EXTRA_CA=$CERT" ]] || { echo "FAIL: wrong export line: $out"; exit 1; }

# openssl parses the cert and it carries the localhost SAN.
openssl x509 -in "$CERT" -noout -text >/dev/null 2>&1 || { echo "FAIL: openssl cannot parse the cert"; exit 1; }
openssl x509 -in "$CERT" -noout -ext subjectAltName 2>/dev/null | grep -F 'DNS:localhost' >/dev/null || { echo "FAIL: cert lacks the localhost SAN"; exit 1; }
# openssl parses the key.
openssl pkey -in "$CERT.key" -noout >/dev/null 2>&1 || { echo "FAIL: openssl cannot parse the key"; exit 1; }
echo "ok: just make-local-cert wrote a cert (SAN localhost) + key openssl parses, printed the export line"
echo "HC1 PASS"
