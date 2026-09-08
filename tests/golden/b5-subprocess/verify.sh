#!/usr/bin/env bash
# B5 acceptance: seahaven's Cmd (PATH search, captured output, exit codes) runs
# over roc:subprocess. OsStr.roc is byte-verbatim; Cmd.roc differs from
# seahaven's ONLY at its two Env.var! call sites plus one appended bridge helper.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b5-subprocess
S=/Users/grayrest/dev/roc/seahaven/platform
cargo build --release -q

cmp -s "$S/OsStr.roc" "$B/components/cmd-lib/OsStr.roc" || { echo "FAIL: OsStr.roc not verbatim"; exit 1; }
# Cmd.roc: exactly 2 changed lines (the env_var_os! bridges) and only appended text otherwise
changed=$(diff "$S/Cmd.roc" "$B/components/cmd-lib/Cmd.roc" | grep -cE '^<' || true)
[[ "$changed" == "2" ]] || { echo "FAIL: Cmd.roc has $changed removed/changed seahaven lines (expected exactly 2)"; diff "$S/Cmd.roc" "$B/components/cmd-lib/Cmd.roc" | grep -E '^<' | head; exit 1; }
echo "ok: OsStr.roc verbatim; Cmd.roc = seahaven's + 2 bridged call sites + an appended helper"

./target/release/hematite compose "$B" >/dev/null
( cd "$B" && ./build.sh app b5 >/dev/null 2>&1 )
if ! _sc=$(./target/release/hematite scan "$B" 2>&1); then echo "FAIL: nm-scan (H0c symbol collision)" >&2; echo "$_sc" >&2; exit 1; fi
set +e; out=$(cd "$B" && ./bin/b5 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: exit $code"; echo "$out"; exit 1; }
want=$'output: hello from a child\nexit-code: 7\npath-search: ok'
[[ "$out" == "$want" ]] || { echo "FAIL: output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: exec_output (PATH-searched echo), exec_exit_code (7), check_available! (ls)"
echo "B5 PASS"
