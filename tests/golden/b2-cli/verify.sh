#!/usr/bin/env bash
# B2 acceptance: a basic-cli hello-world runs UNCHANGED on the main!-compat
# world, and its run!-native twin runs on the run! world; basic-cli's own
# Stdout/Stderr/Stdin/Tty modules are byte-identical to the 0.21 cache.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b2-cli
C=/Users/grayrest/.cache/roc/packages/4rAQg8kUYZ3Vksr4qMQHpaFYNiHSn9GgS7gVxghd1XYV
cargo build --release -q

for m in Stdout Stderr Stdin Tty; do
  cmp -s "$C/$m.roc" "$B/components/stdio-lib/$m.roc" || { echo "FAIL: $m.roc differs from basic-cli 0.21 (must be verbatim)"; exit 1; }
done
echo "ok: Stdout/Stderr/Stdin/Tty byte-identical to basic-cli 0.21 (zero edits)"

check() { # $1 world, $2 app, $3 out
  if ! _b=$(./target/release/hematite build "$B" --world "$1" --app "$2" --out "$3" 2>&1); then echo "FAIL: build $3" >&2; echo "$_b" >&2; exit 1; fi
  local out err code
  set +e
  out=$(cd "$B" && USER=grayrest ./target/hematite/b2-cli/bin/$3 a b 2>/tmp/b2verr); code=$?
  set -e
  err=$(cat /tmp/b2verr)
  [[ $code -eq 0 ]] || { echo "FAIL [$1]: exit $code"; echo "$out"; echo "$err"; exit 1; }
  [[ "$out" == $'hello from basic-cli on hematite\nuser: grayrest\nargs: a,b' ]] || { echo "FAIL [$1]: stdout"; echo "$out"; exit 1; }
  [[ "$err" == "(diagnostic on stderr)" ]] || { echo "FAIL [$1]: stderr"; echo "$err"; exit 1; }
  echo "ok: $1 ($2) -> stdout/stderr/exit as expected"
}
check world.toml app-main b2-main
check world-run.toml app-run b2-run
./target/release/hematite compose "$B" >/dev/null   # leave the committed default world in place
echo "B2 PASS"
