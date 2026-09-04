#!/usr/bin/env bash
# B8 acceptance: the roc:basic-cli world (P14/P15).
#   1. Migration proof: every non-sqlite basic-cli 0.21.0 example `roc check`s
#      with ONLY its platform URL changed (the examples come from the tagged
#      basic-cli repo; the platform is the composed world).
#   2. Runs: the non-interactive, non-network examples build and produce
#      basic-cli's output (argv incl. argv[0], stdin, env, files, dirs, cmds).
#   3. Publish: `hematite publish` emits dist/ with baseline.lock; `hematite
#      tier` classifies a pure-Roc extension as Tier 1.
#   4. Confinement swap: world-confined.toml wires fs-confined; an escaping
#      write is allowed on the baseline and denied on the confined world.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b8-basic-cli
REPO=/Users/grayrest/Repositories/roc-basic-cli
TAG=0.21.0
ROC="${ROC:-$HOME/.bin/roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 90 "$@"; }
[[ -d $REPO/.git ]] || { echo "FAIL: basic-cli repo not at $REPO (needed for the migration proof)"; exit 1; }
cargo build --release -q

./target/release/hematite compose "$B" >/dev/null
( cd "$B" && rm -rf platform/targets && ./build.sh app b8 >/dev/null 2>&1 )

# ---- 1. migration proof (URL swap only) ----
X="$B/examples-run"; rm -rf "$X"; mkdir -p "$X"
pass=0; total=0; failed=""
for f in $(git -C "$REPO" ls-tree --name-only "$TAG" examples/ | grep '\.roc$'); do
  n=$(basename "$f" .roc); [[ $n == sqlite-* ]] && continue
  total=$((total+1)); mkdir -p "$X/$n"
  git -C "$REPO" show "${TAG}:${f}" | perl -pe 's|platform "[^"]+"|platform "../../platform/main.roc"|' > "$X/$n/main.roc"
  if cap "$ROC" check "$X/$n/main.roc" >/dev/null 2>&1; then pass=$((pass+1)); else failed="$failed $n"; fi
done
[[ $pass -eq $total ]] || { echo "FAIL: $pass/$total examples check; failing:$failed"; exit 1; }
echo "ok: $pass/$total basic-cli $TAG examples roc-check with only the platform URL changed"

# ---- 2. runs ----
build() { cap "$ROC" build --output="$B/bin/ex-$1" "$X/$1/main.roc" >/dev/null 2>&1 || { echo "FAIL: build $1"; exit 1; }; }
for n in hello-world hello print dir temp-dir file-size file-read-buffered file-replace file-permissions file-read-write random url command command-line-args stdin-basic stdin-pipe bytes-stdin-stdout env-var path time locale; do build "$n"; done
cd "$X"; printf 'line one\nline two\nline three\n' > LICENSE; cp hello/main.roc main.roc   # 29 bytes, 3 lines
expect() { # name, expected (exact stdout)
  local got; got=$(eval "$2" 2>/dev/null); [[ "$got" == "$3" ]] || { echo "FAIL: $1"; diff <(echo "$3") <(echo "$got") || true; exit 1; }
}
has() { local got; got=$(eval "$2" 2>&1); grep -qF -- "$3" <<<"$got" || { echo "FAIL: $1 (missing: $3)"; echo "$got"; exit 1; }; }
expect hello-world   "../bin/ex-hello-world"            "Hello, World!"
expect hello         "../bin/ex-hello"                  "Hello, friend, from basic-cli!"
expect hello-arg     "../bin/ex-hello Roc"              "Hello, Roc, from basic-cli!"
expect print         "../bin/ex-print"                  $'Hello, world!\nNo newline after me.Foo\nBar\nBaz\n["Foo", "Bar", "Baz"]'
expect file-read-write "../bin/ex-file-read-write"      $'Writing a string to out.txt\nI read the file back. Its contents are: "a string!"'
expect file-replace  "../bin/ex-file-replace"           'After replacing: "Goodbye, World! Goodbye, Roc!"'
expect file-size     "../bin/ex-file-size LICENSE"      "LICENSE is 29 bytes"
expect file-read-buffered "../bin/ex-file-read-buffered" "Done reading file: { bytes_read: 29, lines_read: 3 }"
expect file-permissions "../bin/ex-file-permissions LICENSE" $'LICENSE file permissions:\n    Executable: False\n    Readable: True\n    Writable: True'
expect stdin-basic   "printf 'Ada\nLovelace\n' | ../bin/ex-stdin-basic" $'What\'s your first name?\nWhat\'s your last name?\nHi, Ada Lovelace! \xf0\x9f\x91\x8b'
expect stdin-pipe    "printf abc | ../bin/ex-stdin-pipe" 'This is what you piped in: "abc"'
expect bytes-stdin   "printf wxyz | ../bin/ex-bytes-stdin-stdout" "wxyz"
expect env-var       "EDITOR=vim LETTERS=a,b,c ../bin/ex-env-var" $'Your favorite editor is vim!\nYour favorite letters are: a b c'
expect url           "../bin/ex-url | head -1"          "Request URL: https://api.example.com/v1/search?q=roc+lang&page=1#results"
has dir              "../bin/ex-dir"                    "demo-workspace/src"
has dir-2            "../bin/ex-dir"                    "Workspace cleaned up."
has temp-dir         "../bin/ex-temp-dir"               "The temp dir path is /"
has random           "../bin/ex-random"                 "Random U64 seed is: "
has command          "../bin/ex-command"                "Exit code: 1"
has command-2        "../bin/ex-command"                "BAZ=DUCK"
has args             "../bin/ex-command-line-args héllo" 'UTF-8 argument text: "héllo"'
has path             "../bin/ex-path main.roc"          "Filename: main.roc"
has path-2           "../bin/ex-path main.roc"          "Type: IsFile"
has time             "../bin/ex-time"                   "Completed in "
has locale           "LANG=en_US.UTF-8 ../bin/ex-locale" "application: en-US"
rm -rf demo-workspace out.txt greeting.txt; cd - >/dev/null
echo "ok: 21 examples run with basic-cli's output (argv[0], stdin, env, files, dirs, subprocess, time, locale)"

# ---- 3. publish + tier ----
./target/release/hematite publish "$B" >/dev/null 2>&1
[[ -f "$B/dist/baseline.lock" ]] && grep -q abi_fingerprint "$B/dist/baseline.lock" || { echo "FAIL: publish produced no baseline.lock"; exit 1; }
[[ -f "$B/dist/platform/targets/arm64mac/libtemporal_host.a" ]] || { echo "FAIL: dist lacks archives"; exit 1; }
[[ ! -f "$B/dist/platform/targets/arm64mac/libtestnet_host.a" ]] || { echo "FAIL: test scaffolding leaked into the published baseline"; exit 1; }
./target/release/hematite tier "$B/extension" | grep -q "^Tier 1" || { echo "FAIL: pure-Roc extension not classified Tier 1"; exit 1; }
echo "ok: published dist/ with baseline.lock (no test scaffolding); pure-Roc extension is Tier 1"

# ---- 4. confinement swap ----
( cd "$B" && ./build.sh app-escape b8-escape-open >/dev/null 2>&1 )
[[ "$(cd "$B" && ./bin/b8-escape-open)" == "escape: allowed" ]] || { echo "FAIL: baseline should allow the escaping write"; exit 1; }
./target/release/hematite compose "$B" --world world-confined.toml >/dev/null
( cd "$B" && ./build.sh app-escape b8-escape-confined >/dev/null 2>&1 && ./build.sh app b8-confined >/dev/null 2>&1 )
[[ "$(cd "$B" && ./bin/b8-escape-confined)" == "escape: denied" ]] || { echo "FAIL: confined world should deny the escaping write"; exit 1; }
[[ "$(cd "$B" && ./bin/b8-confined | tail -1)" == 'I read the file back. Its contents are: "a string!"' ]] || { echo "FAIL: confined world should allow in-cwd writes"; exit 1; }
rm -f "$B/../b8-escape.txt" "$B/out.txt"
./target/release/hematite compose "$B" >/dev/null   # leave the committed default world composed
echo "ok: same app, fs-confined wired instead of fs-unconfined: escape denied, cwd writes allowed"
echo "B8 PASS"
