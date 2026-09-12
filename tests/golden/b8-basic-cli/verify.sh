#!/usr/bin/env bash
# B8 acceptance: the roc:basic-cli world (P14/P15).
#   1. Migration proof: every non-sqlite basic-cli 0.21.0 example `roc check`s
#      with ONLY its platform URL changed (the examples come from the tagged
#      basic-cli repo; the platform is the composed world).
#   2. Runs: the non-interactive, non-network examples build and produce
#      basic-cli's output (argv incl. argv[0], stdin, env, files, dirs, cmds).
#   3. Publish: `trantor publish` emits target/trantor/b8-basic-cli/dist/ with baseline.lock; `trantor
#      tier` classifies a pure-Roc extension as Tier 1.
#   4. Confinement swap: world-confined.toml wires fs-confined; an escaping
#      write is allowed on the baseline and denied on the confined world.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b8-basic-cli
PKG=$PWD/../trantor-cli
NET=$PWD/../trantor-net
TERMINAL=$PWD/../trantor-terminal
[[ -f "$PKG/package.toml" && -f "$NET/package.toml" && -f "$TERMINAL/package.toml" ]] || {
	echo "SKIP: this fixture consumes the trantor-cli, trantor-net and trantor-terminal packages,"
	echo "      which are not all checked out beside this repo."
	exit 0
}
REPO=/Users/grayrest/Repositories/roc-basic-cli
TAG=0.21.0
ROC="${ROC:-$HOME/.bin/roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 90 "$@"; }
[[ -d $REPO/.git ]] || { echo "FAIL: basic-cli repo not at $REPO (needed for the migration proof)"; exit 1; }
cargo build --release -q

( cd "$B" && rm -rf target/trantor/b8-basic-cli/platform/targets )
if ! _b=$(./target/release/trantor build "$B" --app app --out b8 2>&1); then echo "FAIL: build b8" >&2; echo "$_b" >&2; exit 1; fi

# ---- 1. migration proof (URL swap only) ----
# The two http examples (http-client, http-simple) are EXCLUDED here: choosing a
# streaming Http.Response (H6) means they no longer pure-URL-swap — they take a
# small, enumerated streaming adaptation and are built + RUN in the HC3 section
# below. Every other non-sqlite example still migrates by URL alone.
X="$B/examples-run"; rm -rf "$X"; mkdir -p "$X"
pass=0; total=0; failed=""
for f in $(git -C "$REPO" ls-tree --name-only "$TAG" examples/ | grep '\.roc$'); do
  n=$(basename "$f" .roc); [[ $n == sqlite-* ]] && continue
  [[ $n == http-client || $n == http-simple ]] && continue   # adapted + run in HC3
  total=$((total+1)); mkdir -p "$X/$n"
  git -C "$REPO" show "${TAG}:${f}" | perl -pe 's|platform "[^"]+"|platform "../../target/trantor/b8-basic-cli/platform/main.roc"|' > "$X/$n/main.roc"
  if cap "$ROC" check "$X/$n/main.roc" >/dev/null 2>&1; then pass=$((pass+1)); else failed="$failed $n"; fi
done
# Guard against a vacuous 0/0 pass (wrong tag, moved examples/, detached repo):
# basic-cli 0.21.0 ships ~26 non-sqlite, non-http examples.
[[ $total -ge 20 ]] || { echo "FAIL: only $total example(s) found at $TAG (expected >=20); migration proof would be vacuous"; exit 1; }
# The floor alone does not prove the two Tty examples are in the count — it held
# at 24 without them. Name them.
for n in tty terminal-app-snake; do
  [[ -f "$X/$n/main.roc" ]] || { echo "FAIL: $n is not among the checked examples"; exit 1; }
done
[[ $pass -eq $total ]] || { echo "FAIL: $pass/$total examples check; failing:$failed"; exit 1; }
echo "ok: $pass/$total basic-cli $TAG non-http examples roc-check with only the platform URL changed"

# ---- 2. runs ----
build() { cap "$ROC" build --output="$B/target/trantor/b8-basic-cli/bin/ex-$1" "$X/$1/main.roc" >/dev/null 2>&1 || { echo "FAIL: build $1"; exit 1; }; }
for n in hello-world hello print dir temp-dir file-size file-read-buffered file-replace file-permissions file-read-write random url command command-line-args stdin-basic stdin-pipe bytes-stdin-stdout env-var path time locale; do build "$n"; done
cd "$X"; printf 'line one\nline two\nline three\n' > LICENSE; cp hello/main.roc main.roc   # 29 bytes, 3 lines
expect() { # name, expected (exact stdout)
  local got; got=$(eval "$2" 2>/dev/null); [[ "$got" == "$3" ]] || { echo "FAIL: $1"; diff <(echo "$3") <(echo "$got") || true; exit 1; }
}
has() { local got; got=$(eval "$2" 2>&1); grep -qF -- "$3" <<<"$got" || { echo "FAIL: $1 (missing: $3)"; echo "$got"; exit 1; }; }
expect hello-world   "../target/trantor/b8-basic-cli/bin/ex-hello-world"            "Hello, World!"
expect hello         "../target/trantor/b8-basic-cli/bin/ex-hello"                  "Hello, friend, from basic-cli!"
expect hello-arg     "../target/trantor/b8-basic-cli/bin/ex-hello Roc"              "Hello, Roc, from basic-cli!"
expect print         "../target/trantor/b8-basic-cli/bin/ex-print"                  $'Hello, world!\nNo newline after me.Foo\nBar\nBaz\n["Foo", "Bar", "Baz"]'
expect file-read-write "../target/trantor/b8-basic-cli/bin/ex-file-read-write"      $'Writing a string to out.txt\nI read the file back. Its contents are: "a string!"'
expect file-replace  "../target/trantor/b8-basic-cli/bin/ex-file-replace"           'After replacing: "Goodbye, World! Goodbye, Roc!"'
expect file-size     "../target/trantor/b8-basic-cli/bin/ex-file-size LICENSE"      "LICENSE is 29 bytes"
expect file-read-buffered "../target/trantor/b8-basic-cli/bin/ex-file-read-buffered" "Done reading file: { bytes_read: 29, lines_read: 3 }"
expect file-permissions "../target/trantor/b8-basic-cli/bin/ex-file-permissions LICENSE" $'LICENSE file permissions:\n    Executable: False\n    Readable: True\n    Writable: True'
expect stdin-basic   "printf 'Ada\nLovelace\n' | ../target/trantor/b8-basic-cli/bin/ex-stdin-basic" $'What\'s your first name?\nWhat\'s your last name?\nHi, Ada Lovelace! \xf0\x9f\x91\x8b'
expect stdin-pipe    "printf abc | ../target/trantor/b8-basic-cli/bin/ex-stdin-pipe" 'This is what you piped in: "abc"'
expect bytes-stdin   "printf wxyz | ../target/trantor/b8-basic-cli/bin/ex-bytes-stdin-stdout" "wxyz"
expect env-var       "EDITOR=vim LETTERS=a,b,c ../target/trantor/b8-basic-cli/bin/ex-env-var" $'Your favorite editor is vim!\nYour favorite letters are: a b c'
expect url           "../target/trantor/b8-basic-cli/bin/ex-url | head -1"          "Request URL: https://api.example.com/v1/search?q=roc+lang&page=1#results"
has dir              "../target/trantor/b8-basic-cli/bin/ex-dir"                    "demo-workspace/src"
has dir-2            "../target/trantor/b8-basic-cli/bin/ex-dir"                    "Workspace cleaned up."
has temp-dir         "../target/trantor/b8-basic-cli/bin/ex-temp-dir"               "The temp dir path is /"
has random           "../target/trantor/b8-basic-cli/bin/ex-random"                 "Random U64 seed is: "
has command          "../target/trantor/b8-basic-cli/bin/ex-command"                "Exit code: 1"
has command-2        "../target/trantor/b8-basic-cli/bin/ex-command"                "BAZ=DUCK"
has args             "../target/trantor/b8-basic-cli/bin/ex-command-line-args héllo" 'UTF-8 argument text: "héllo"'
has path             "../target/trantor/b8-basic-cli/bin/ex-path main.roc"          "Filename: main.roc"
has path-2           "../target/trantor/b8-basic-cli/bin/ex-path main.roc"          "Type: IsFile"
has time             "../target/trantor/b8-basic-cli/bin/ex-time"                   "Completed in "
has locale           "LANG=en_US.UTF-8 ../target/trantor/b8-basic-cli/bin/ex-locale" "application: en-US"
rm -rf demo-workspace out.txt greeting.txt; cd - >/dev/null
echo "ok: 21 examples run with basic-cli's output (argv[0], stdin, env, files, dirs, subprocess, time, locale)"

# ---- HC3: streaming Http; the http examples RUN against the in-process testnet ----
# Http.Response is streaming now (H6): send! returns a body InputStream,
# read_body_to_end! collects it, to_http_response! bridges to roc-lang/http's
# eager Response. The two http examples adapt with a handful of streaming lines
# (server start + streaming accessors) and run against the testnet HTTP server
# (basic-cli's ci endpoints, in-process on :9000).
grep -q 'ureq' "$NET/components/http-host/Cargo.toml" || { echo "FAIL: b8 http-host is not over ureq"; exit 1; }
grep -q 'body_stream' "$NET/interfaces/sync-http/HttpHost.roc" || { echo "FAIL: HttpHost.Response is not streaming"; exit 1; }
grep -q 'read_body_to_end!' "$NET/components/net-lib/Http.roc" || { echo "FAIL: Http lacks read_body_to_end!"; exit 1; }
grep -q 'to_http_response' "$NET/components/net-lib/Http.roc" || { echo "FAIL: Http lacks the to_http_response! bridge"; exit 1; }
# The adaptation vs the upstream example is a handful of streaming lines only.
swap() { git -C "$REPO" show "${TAG}:examples/$1.roc" | perl -pe 's|platform "[^"]+"|platform "../target/trantor/b8-basic-cli/platform/main.roc"|'; }
for pair in "http-simple 6" "http-client 9"; do
  set -- $pair; n=$1; bound=$2
  d=$(diff <(swap "$n") "$B/http-examples/$n.roc" | grep -c '^[<>]' || true)
  [[ "$d" -le "$bound" ]] || { echo "FAIL: $n adaptation is $d changed lines (> $bound); should be a small streaming edit"; diff <(swap "$n") "$B/http-examples/$n.roc" || true; exit 1; }
done
echo "ok: http examples adapt to streaming in a handful of lines (server start + streaming accessors)"

for n in http-client http-simple http-bridge; do
  cap "$ROC" build --output="$B/target/trantor/b8-basic-cli/bin/$n" "$B/http-examples/$n.roc" >/dev/null 2>&1 || { echo "FAIL: build $n"; exit 1; }
done
hc=$(cd "$B" && ./target/trantor/b8-basic-cli/bin/http-client 2>/dev/null || true)
want_hc=$'I received \'Hello from the test server!\' from the server.\nThe json I received was: { foo: "json-root" }\nsend! returned status 200.\nsend_json! echoed: { foo: "Hello Json!" }.\ninvalid JSON was rejected.\ninvalid UTF-8 was rejected.\ninvalid request URL was rejected.'
[[ "$hc" == "$want_hc" ]] || { echo "FAIL: http-client output"; diff <(echo "$want_hc") <(echo "$hc") || true; exit 1; }
hs=$(cd "$B" && ./target/trantor/b8-basic-cli/bin/http-simple 2>/dev/null || true)
want_hs=$'I received \'Hello from the test server!\' from the server.\nThe json I received was: { foo: "json-root" }\nResponse body:\n<html><body>hi</body></html>'
[[ "$hs" == "$want_hs" ]] || { echo "FAIL: http-simple output"; diff <(echo "$want_hs") <(echo "$hs") || true; exit 1; }
hb=$(cd "$B" && ./target/trantor/b8-basic-cli/bin/http-bridge 2>/dev/null || true)
[[ "$hb" == "bridge: 200 Hello from the test server!" ]] || { echo "FAIL: to_http_response! bridge (got: $hb)"; exit 1; }
echo "ok: http-client + http-simple run streaming against the testnet; to_http_response! round-trips into roc-lang/http Response"

for n in http-client http-simple http-bridge; do
  g=$(cd "$B" && TRANTOR_ALLOC_GAUGE=1 ./target/trantor/b8-basic-cli/bin/$n 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
  grep -q 'live=0' <<<"$g" || { echo "FAIL: $n leaked heap allocations: $g"; exit 1; }
done
echo "ok: http examples drop-balance across the streaming paths (alloc-gauge live=0)"

# ---- 2a. Env.set_cwd! propagates to the subprocess working directory ----
cap "$ROC" build --output="$B/target/trantor/b8-basic-cli/bin/ex-cwd" "$B/cwd-app/main.roc" >/dev/null 2>&1 || { echo "FAIL: build cwd-app"; exit 1; }
cwdout=$(cd "$B" && ./target/trantor/b8-basic-cli/bin/ex-cwd 2>/dev/null)
grep -q '^before: /usr$' <<<"$cwdout" && { echo "FAIL: cwd test vacuous (process cwd is already /usr)"; exit 1; }
grep -q '^after: /usr$'  <<<"$cwdout" || { echo "FAIL: subprocess did not honor Env.set_cwd! (want 'after: /usr'):"; echo "$cwdout"; exit 1; }
echo "ok: Env.set_cwd! propagates to subprocess cwd (child pwd = /usr, process cwd unchanged)"

# ---- 2b. drop-balance gauge (env-gated alloc counter) ----
# resource live() counts handles; it cannot see leaked RocStr/RocList DATA (the
# B0 owned-arg leaks). TRANTOR_ALLOC_GAUGE makes the driver print allocs/
# deallocs at exit; a heap-data leak shows live>0. gauge-app pushes a RUNTIME-
# built (heap, not static-literal) string through an owned-RocStr host arg
# (Env.set_cwd! -> Cell.put!); ex-file-read-write exercises fs/streams/
# resources. Both must drop-balance to live=0. (The subprocess/http element
# paths can't be gauged: the pinned compiler segfaults on a runtime Str into
# Cmd.args, and small inline args don't heap-allocate -- see gauge-app.)
cap "$ROC" build --output="$B/target/trantor/b8-basic-cli/bin/ex-gauge" "$B/gauge-app/main.roc" >/dev/null 2>&1 || { echo "FAIL: build gauge-app"; exit 1; }
balance() { # label ; env/args... (binary run from $B under the gauge)
  local label="$1"; shift
  local g; g=$( (cd "$B" && TRANTOR_ALLOC_GAUGE=1 "$@") 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
  [[ -n "$g" ]] || { echo "FAIL: $label emitted no gauge line (driver not instrumented?)"; exit 1; }
  local allocs live; allocs=$(sed -E 's/.*allocs=([0-9]+).*/\1/' <<<"$g"); live=$(sed -E 's/.*live=(-?[0-9]+).*/\1/' <<<"$g")
  [[ "${allocs:-0}" -gt 0 ]] || { echo "FAIL: $label saw no allocations, gauge is vacuous: $g"; exit 1; }
  [[ "$live" == "0" ]] || { echo "FAIL: $label leaked Roc heap allocations: $g"; exit 1; }
  echo "ok: $label drop-balanced ($g)"
}
balance "owned RocStr host arg (set_cwd/Cwd)" env GAUGE_SEED="a-heap-seed-string-well-over-twenty-three-bytes-long-for-sure" ./target/trantor/b8-basic-cli/bin/ex-gauge
balance "fs read/write + streams + resources" ./target/trantor/b8-basic-cli/bin/ex-file-read-write
# gauge OFF prints nothing (env-gated):
[[ -z "$( (cd "$B" && GAUGE_SEED=x-well-over-twenty-three-bytes-of-seed-value ./target/trantor/b8-basic-cli/bin/ex-gauge >/dev/null) 2>&1 | grep '^\[alloc-gauge\]' || true)" ]] || { echo "FAIL: gauge printed while disabled"; exit 1; }
echo "ok: gauge silent unless TRANTOR_ALLOC_GAUGE is set"

# ---- 2c. the Tty examples, run over a real pty (trantor-terminal) ----
# Checking is not enough for raw mode: a Tty whose leaves do nothing checks
# fine, and did, for months. So both run on a pty, keys are sent only once a
# frame has been drawn, and the terminal must be raw while they read and
# restored once they exit. pty-script comes from trantor-terminal's own harness.
cap "$ROC" build --output="$B/target/trantor/b8-basic-cli/bin/ex-tty" "$X/tty/main.roc" >/dev/null 2>&1 || { echo "FAIL: build tty"; exit 1; }
cap "$ROC" build --output="$B/target/trantor/b8-basic-cli/bin/ex-snake" "$X/terminal-app-snake/main.roc" >/dev/null 2>&1 || { echo "FAIL: build terminal-app-snake"; exit 1; }
CARGO_TARGET_DIR="$PWD/$B/target/pty-harness" cargo build --release -q --manifest-path "$TERMINAL/tests/pty/harness/Cargo.toml" --bin pty-script \
  || { echo "FAIL: build trantor-terminal's pty-script"; exit 1; }
PTY="$PWD/$B/target/pty-harness/release/pty-script"
out=$(cap "$PTY" "$B/target/trantor/b8-basic-cli/bin/ex-tty" "wait=1:Press one key" raw "send=a" "wait=1:Read 1 byte" exit=0 2>&1) \
  || { echo "FAIL: tty example over a pty: $out"; exit 1; }
echo "ok: tty example — raw while it reads one key, restored when it exits"
out=$(cap "$PTY" "$B/target/trantor/b8-basic-cli/bin/ex-snake" "wait=1:Score:" raw "send=d" "wait=2:Score:" "send=q" "wait=1:Game Over" exit=0 2>&1) \
  || { echo "FAIL: terminal-app-snake over a pty: $out"; exit 1; }
echo "ok: terminal-app-snake — draws, moves on a key, quits on q, terminal restored"

# ---- 3. publish + tier ----
./target/release/trantor publish "$B" >/dev/null 2>&1
[[ -f "$B/target/trantor/b8-basic-cli/dist/baseline.lock" ]] && grep -q abi_fingerprint "$B/target/trantor/b8-basic-cli/dist/baseline.lock" || { echo "FAIL: publish produced no baseline.lock"; exit 1; }
[[ -f "$B/target/trantor/b8-basic-cli/dist/platform/targets/arm64mac/libsubprocess_host.a" ]] || { echo "FAIL: dist lacks archives"; exit 1; }
[[ ! -f "$B/target/trantor/b8-basic-cli/dist/platform/targets/arm64mac/libtestnet_host.a" ]] || { echo "FAIL: test scaffolding leaked into the published baseline"; exit 1; }
# Captured, not piped into `grep -q`: grep exits on the first match, and the
# "(N component(s) examined)" line `tier` prints after it then hits a closed
# pipe, panics, and `pipefail` turns that into a failure of THIS check.
_t=$(./target/release/trantor tier "$B/extension")
[[ "$_t" == Tier\ 1:* ]] || { echo "FAIL: pure-Roc extension not classified Tier 1 — got: $_t"; exit 1; }
echo "ok: published target/trantor/b8-basic-cli/dist/ with baseline.lock (no test scaffolding); pure-Roc extension is Tier 1"

# ---- 4. confinement swap ----
if ! _b=$(./target/release/trantor build "$B" --app app-escape --out b8-escape-open 2>&1); then echo "FAIL: build b8-escape-open" >&2; echo "$_b" >&2; exit 1; fi
[[ "$(cd "$B" && ./target/trantor/b8-basic-cli/bin/b8-escape-open)" == "escape: allowed" ]] || { echo "FAIL: baseline should allow the escaping write"; exit 1; }
if ! _b=$(./target/release/trantor build "$B" --world world-confined.toml --app app-escape --out b8-escape-confined 2>&1); then echo "FAIL: build b8-escape-confined" >&2; echo "$_b" >&2; exit 1; fi
if ! _b=$(./target/release/trantor build "$B" --world world-confined.toml --app app --out b8-confined 2>&1); then echo "FAIL: build b8-confined" >&2; echo "$_b" >&2; exit 1; fi
[[ "$(cd "$B" && ./target/trantor/b8-basic-cli/bin/b8-escape-confined)" == "escape: denied" ]] || { echo "FAIL: confined world should deny the escaping write"; exit 1; }
[[ "$(cd "$B" && ./target/trantor/b8-basic-cli/bin/b8-confined | tail -1)" == 'I read the file back. Its contents are: "a string!"' ]] || { echo "FAIL: confined world should allow in-cwd writes"; exit 1; }
rm -f "$B/../b8-escape.txt" "$B/out.txt"
if ! _b=$(./target/release/trantor build "$B" --app app --out b8 2>&1); then echo "FAIL: build b8 (default)" >&2; echo "$_b" >&2; exit 1; fi   # leave the committed default world composed + built
echo "ok: same app, fs-confined wired instead of fs-unconfined: escape denied, cwd writes allowed"
echo "B8 PASS"
