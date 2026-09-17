#!/usr/bin/env bash
# B7 acceptance: trantor-temporal composed on top of trantor-cli, with no
# component of B7's own.
#
# What it proves that the package's own gate does not:
#   1. every resource the package hands out is freed (the alloc gauge),
#   2. temporal_rs lives in temporal-host's archive and NO other (H0c),
#   3. a world WITHOUT the package links none of temporal_rs.
# The package's gate has a module-level negative control but no archive check,
# no link-level one, and no drop gauge.
#
# Behaviour checks are deliberately thin. The package's own verify.sh pins
# dozens; duplicating them is how B7's old vendored copy drifted.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b7-temporal
cargo build --release -q

if ! _b=$(./target/release/trantor build "$B" --app app --out b7 2>&1); then echo "FAIL: build b7" >&2; echo "$_b" >&2; exit 1; fi
out=$(cd "$B" && ./target/trantor/b7-temporal/bin/b7 2>/dev/null) || { echo "FAIL: b7 exited nonzero"; echo "$out"; exit 1; }
want=$'date-add: 2024-02-29\nzdt-ny: 2024-03-10T01:30:00-05:00[America/New_York]\nzdt-tokyo: 2024-03-10T15:30:00+09:00[Asia/Tokyo]\nsame-instant: yes\nday-of-week: 4'
[[ "$out" == "$want" ]] || { echo "FAIL: output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }

# (1) Drop balance. This used to be the exit code, set from a `Gauge.live!`
#     component B7 vendored for the purpose; the platform's alloc gauge counts
#     every allocation, not only handles, and needs nothing in the app.
g=$(cd "$B" && TRANTOR_ALLOC_GAUGE=1 ./target/trantor/b7-temporal/bin/b7 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
[[ -n "$g" ]] || { echo "FAIL: the alloc gauge printed nothing"; exit 1; }
allocs=$(sed -n 's/.*allocs=\([0-9]*\).*/\1/p' <<<"$g")
# live=0 is vacuous if nothing was ever allocated.
[[ "${allocs:-0}" -gt 0 ]] || { echo "FAIL: the gauge saw no allocations, so live=0 proves nothing ($g)"; exit 1; }
grep -q 'live=0' <<<"$g" || { echo "FAIL: temporal resources leaked: $g"; exit 1; }
echo "ok: trantor-temporal composes on trantor-cli; NY/Tokyo conversion; $g"

# (2) H0c: temporal_rs natives live in libtemporal_host.a and nowhere else.
T="$B/target/trantor/b7-temporal/platform/targets/arm64mac"
ar t "$T/libtemporal_host.a" | grep "^temporal_rs-" >/dev/null || { echo "FAIL: libtemporal_host.a has no temporal_rs symbols"; exit 1; }
n=0
for a in "$T"/lib*.a; do
  n=$((n+1))
  [[ "$(basename "$a")" == "libtemporal_host.a" ]] && continue
  if ar t "$a" | grep "^temporal_rs-" >/dev/null; then echo "FAIL: $(basename "$a") also carries temporal_rs"; exit 1; fi
done
[[ "$n" -gt 1 ]] || { echo "FAIL: only $n archive(s) staged, so 'no other archive' is vacuous"; exit 1; }
echo "ok: temporal_rs in libtemporal_host.a only, across $n archives (H0c)"

# (3) A world WITHOUT the package carries none of it.
#
#     Asserted on the STAGED ARCHIVES, not on the linked binary. The old check
#     ran `nm` on b4's executable and grepped for temporal_rs — and so would
#     any binary whose app never called a hosted temporal function, package or
#     no package, because the linker dead-strips an unused temporal-host.
#     Measured: a world that includes trantor-temporal but only calls the
#     pure-Roc `plain_date` links 0 temporal_rs symbols, against 97 for one
#     that makes a hosted call. That check could not tell "absent" from
#     "present but unused". Staging is decided by composition, not by use, so
#     it can: add the package to this world and libtemporal_host.a is staged
#     and one archive carries temporal_rs, whether or not the app touches it.
#
#     The world lives in a directory NAMED baseline: the composed platform's
#     path is keyed on the world directory's name, not `[world] name` (D-H7-38).
# mktemp alone first: inside `cd "$(mktemp -d)"` a failed mktemp leaves `cd ""`,
# which macOS /bin/bash and Ubuntu's bash accept as staying put, so SCRATCH
# became the repo root, which the EXIT trap then deleted.
SCRATCH=$(mktemp -d); SCRATCH=$(cd "$SCRATCH" && pwd -P); trap 'rm -rf "$SCRATCH"' EXIT
S="$SCRATCH/baseline"
mkdir -p "$S/app"
printf '[world]\nname = "baseline"\n\n[deps]\ntrantor-cli = { path = "%s" }\n' "$PWD/../trantor-cli" > "$S/world.toml"
cat > "$S/app/main.roc" <<'ROC'
app [main!] { pf: platform "../target/trantor/baseline/platform/main.roc" }
import pf.OsStr exposing [OsStr]
import pf.Stdout
main! : List(OsStr) => Try({}, _)
main! = |_args| Stdout.line!("baseline")
ROC
./target/release/trantor build "$S" --app app --out base >/dev/null 2>&1 || { echo "FAIL: build the baseline-only world"; exit 1; }
BA="$S/target/trantor/baseline/platform/targets/arm64mac"
bn=0; carriers=0
for a in "$BA"/lib*.a; do
  bn=$((bn+1))
  if ar t "$a" | grep "^temporal_rs-" >/dev/null; then carriers=$((carriers+1)); fi
done
# Nothing staged would make "no archive carries it" vacuous.
[[ "$bn" -gt 1 ]] || { echo "FAIL: the baseline world staged $bn archive(s) — the check below would be vacuous"; exit 1; }
[[ ! -f "$BA/libtemporal_host.a" ]] || { echo "FAIL: a world without trantor-temporal staged libtemporal_host.a"; exit 1; }
[[ "$carriers" == 0 ]] || { echo "FAIL: $carriers archive(s) in a world without trantor-temporal carry temporal_rs"; exit 1; }
echo "ok: a trantor-cli-only world stages no temporal archive and no temporal_rs, across $bn archives"
echo "B7 PASS"
