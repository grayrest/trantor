# trantor dev recipes.

# Generate a self-signed localhost cert for TRANTOR_HTTP_EXTRA_CA (H13/H14).
# Writes the cert PEM to <cert-path> and the key to <cert-path>.key, and prints
# the `export TRANTOR_HTTP_EXTRA_CA=<cert-path>` line to wire it into the HTTP
# client. local-cert is a standalone crate (its crypto deps stay out of the
# trantor tool's build), so it's run by --manifest-path, not `-p`.
make-local-cert cert-path:
    @command -v cargo >/dev/null 2>&1 || { echo "make-local-cert: needs cargo on PATH (install Rust)"; exit 1; }
    cargo run --quiet --release --manifest-path tools/local-cert/Cargo.toml -- {{cert-path}}
