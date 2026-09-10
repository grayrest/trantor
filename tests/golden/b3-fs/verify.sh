#!/usr/bin/env bash
# B3 acceptance: the capability filesystem over three worlds.
#  - unconfined: basic-cli-shaped File/Path/Dir/Env.cwd code runs; /etc/hosts readable
#  - confined:   identical app, out-of-preopen path is a capability error (denied)
#  - ospath:     roc:os-path exposed AS `Path` by D14 rename; app unchanged, same output
# Exit code == live Descriptors at the end (0 = drop-balanced) in every world.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b3-fs
cargo build --release -q

# the bug that bit the first draft: OsPath.roc must really be the lossless impl
grep -q '^OsPath := \[Text(Str), Raw(List(U8))\]' "$B/components/os-path/OsPath.roc" || { echo "FAIL: OsPath.roc is not the lossless impl"; exit 1; }
stray=$(grep -vE '^\s*##' "$B/components/os-path/OsPath.roc" | grep -cE '\bPath\b' || true)
[[ "$stray" == "0" ]] || { echo "FAIL: OsPath.roc has $stray stray bare Path tokens"; exit 1; }
echo "ok: roc:os-path is the lossless [Text|Raw] impl, no stray Path tokens"

expected_common=$'read: one\ntwo\nkind: file\nlist: note.txt\ncwd-ok: yes'
run() { # $1 world  $2 out  $3 expected escape line  $4 expected Path decl
  ./target/release/hematite compose "$B" --world "$1" >/dev/null
  local decl; decl=$(grep -m1 -E '^(Path|OsPath) :=' "$B/target/hematite/b3-fs/platform/Path.roc")
  [[ "$decl" == "$4"* ]] || { echo "FAIL [$1]: platform Path.roc decl is '$decl', want '$4'"; exit 1; }
  if ! _b=$(./target/release/hematite build "$B" --world "$1" --app app --out "$2" 2>&1); then echo "FAIL: build $2" >&2; echo "$_b" >&2; exit 1; fi
  local out code
  set +e; out=$(cd "$B" && ./target/hematite/b3-fs/bin/$2 2>/dev/null); code=$?; set -e
  [[ $code -eq 0 ]] || { echo "FAIL [$1]: exit $code (live descriptors leaked)"; echo "$out"; exit 1; }
  [[ "$out" == "$expected_common"$'\n'"escape: $3" ]] || { echo "FAIL [$1]: output"; echo "$out"; exit 1; }
  echo "ok: $1 -> $4… ; escape: $3 ; live=0"
}
run world.toml          b3-unconfined escaped 'Path := [Path(Str)]'
run world-confined.toml b3-confined   denied  'Path := [Path(Str)]'
run world-ospath.toml   b3-ospath     escaped 'Path := [Text(Str), Raw(List(U8))]'
./target/release/hematite compose "$B" >/dev/null   # leave the committed default world composed
rm -rf "$B/b3-scratch"
echo "B3 PASS"
