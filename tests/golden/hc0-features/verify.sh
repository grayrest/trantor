#!/usr/bin/env bash
# HC0 acceptance: the composer's per-component Cargo-features knob (H12).
#   1. Compose the default world (features = ["extra"]): the generated
#      components/marker/Cargo.toml carries `default = ["extra"]`, the archive
#      built from it exports the `extra`-gated symbol, and the app runs.
#   2. Compose world-noextra.toml (default_features = false): the generated
#      Cargo.toml carries `default = []` and the archive drops the gated symbol.
#   The build divergence — same source, same manifest field flipped — is what a
#   features knob has to prove. Idempotent: composing the default world twice is
#   byte-identical (the committed marker Cargo.toml already reads default=extra).
set -euo pipefail
cd "$(dirname "$0")/../../.."
H=tests/golden/hc0-features
source tests/golden/host-target.sh
MARK="$H/target/trantor/hc0-features/components/marker/Cargo.toml"
ARCH="$H/target/trantor/hc0-features/platform/targets/$HOST_TARGET/libmarker.a"
cargo build --release -q

feat_line() { grep -E '^default = ' "$MARK"; }
# `{ nm ... || true; }`: Xcode's nm exits 1 on this archive (it can't read some
# rust-LLVM CGU object members) yet still prints the symbols it CAN read,
# including ours; under `pipefail` that non-zero exit would fail the pipeline
# regardless of the grep, so tolerate it. `grep >/dev/null` (not `-q`) so grep
# reads to EOF rather than SIGPIPE-ing nm.
has_sym() { { nm "$ARCH" 2>/dev/null || true; } | grep extra_marker >/dev/null; }

# ---- 1. feature ON (default world) ----
./target/release/trantor compose "$H" >/dev/null
# Idempotent: the composed marker Cargo.toml matches the committed source.
git diff --quiet -- "$MARK" || { echo "FAIL: composing the default world changed the committed marker Cargo.toml (not idempotent):"; git --no-pager diff -- "$MARK"; exit 1; }
[[ "$(feat_line)" == 'default = ["extra"]' ]] || { echo "FAIL: default world should map features=[extra] to 'default = [\"extra\"]', got: $(feat_line)"; exit 1; }
if ! _b=$(./target/release/trantor build "$H" --app app --out hc0 2>&1); then echo "FAIL: build hc0 (feature on)" >&2; echo "$_b" >&2; exit 1; fi
has_sym || { echo "FAIL: extra-gated symbol missing with the feature ON"; exit 1; }
[[ "$(cd "$H" && ./target/trantor/hc0-features/bin/hc0)" == "ping: hc0" ]] || { echo "FAIL: app did not run"; exit 1; }
echo "ok: features=[\"extra\"] -> default=[\"extra\"], gated symbol present, app runs"

# ---- 2. feature OFF ----
./target/release/trantor compose "$H" --world world-noextra.toml >/dev/null
[[ "$(feat_line)" == 'default = []' ]] || { echo "FAIL: default_features=false should map to 'default = []', got: $(feat_line)"; exit 1; }
if ! _b=$(./target/release/trantor build "$H" --world world-noextra.toml --app app --out hc0 2>&1); then echo "FAIL: build hc0 (feature off)" >&2; echo "$_b" >&2; exit 1; fi
! has_sym || { echo "FAIL: extra-gated symbol still present with the feature OFF (knob did not take effect)"; exit 1; }
echo "ok: default_features=false -> default=[], gated symbol absent (same source, one manifest field flipped)"

# ---- leave the committed default world composed + built ----
if ! _b=$(./target/release/trantor build "$H" --app app --out hc0 2>&1); then echo "FAIL: build hc0 (default)" >&2; echo "$_b" >&2; exit 1; fi
git diff --quiet -- "$MARK" || { echo "FAIL: did not restore the committed marker Cargo.toml"; exit 1; }
echo "HC0 PASS"
