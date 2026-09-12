#!/usr/bin/env bash
# U1 acceptance: the whole front door, in the order a user meets it.
#
# The scenario is the one the plan was written around — do the work by shelling
# out first, then move it into a Rust library — with `tr` standing in for
# imagemagick and a local crate for the image crate, so the gate needs no
# network and no installed tools beyond the toolchain.
#
# The property under test is NOT that both halves work. It is that the app does
# not know which half it got: the same interface, the same Roc source, byte for
# byte, across a subprocess implementation and a library one. If moving the work
# changes the app, the interface boundary did not hold and the walkthrough
# proved nothing.
set -euo pipefail
cd "$(dirname "$0")/../../.."
REPO=$PWD
FIX=$REPO/tests/golden/u1-front-door
# `pwd -P`, because mktemp hands back /var/folders/... and /var is a symlink to
# /private/var. `cargo add --path` writes a relative path counted from the
# resolved root, and cargo then resolves it from the resolved root too — so the
# alias works everywhere except where the two disagree. Nothing to do with
# trantor; the harness just has to use a real path.
T=$(cd "$(mktemp -d)" && pwd -P); P=$T/resize
trap 'rm -rf "$T"' EXIT
cargo build --release -q
TR=$REPO/target/release/trantor

# Every command must exit 0 and print no error text (D-U1-15, replacing the old
# file-count metric). Errors are what a first-time user actually hits.
step() {
	local label="$1"; shift
	local log="$T/log"
	if ! "$@" >"$log" 2>&1; then
		echo "FAIL: $label exited nonzero"; sed 's/^/      | /' "$log" | tail -20; exit 1
	fi
	if grep -qiE '^error|error:' "$log"; then
		echo "FAIL: $label printed error text"; grep -iE '^error|error:' "$log" | head -5; exit 1
	fi
	echo "ok: $label"
}

# ---- 1. a project exists and works the moment it is made --------------------
step "trantor new" "$TR" new "$P" --cli --from "$FIX/base"
[[ -f "$P/world.toml" && -f "$P/app/main.roc" && -f "$P/Cargo.toml" && -f "$P/.gitignore" ]] \
	|| { echo "FAIL: new did not scaffold the four files"; exit 1; }

# ---- 2. the app, written once and never touched again -----------------------
cat > "$P/app/main.roc" <<'ROC'
app [main!] { pf: platform "../target/trantor/resize/platform/main.roc" }

import pf.Upper

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Upper.shout!("resize me")
	Ok({})
}
ROC
APP_BEFORE=$(shasum "$P/app/main.roc" | cut -d' ' -f1)

# ---- 3. the user's own interface --------------------------------------------
step "trantor new-interface" "$TR" new-interface "$P" upper
# Editing the scaffolded declaration is the intended next step; the scaffold
# says so.
cat > "$P/interfaces/upper/Upper.roc" <<'ROC'
Upper :: [].{
	shout! : Str => {}
}
ROC
sed -i '' 's/do_it!/shout!/; s/do_it/shout/' "$P/interfaces/upper/interface.toml"

step "trantor build --platform-only (glue)" "$TR" build "$P" --platform-only

# ---- 4. the signature, generated rather than guessed ------------------------
step "trantor interface-stub" "$TR" interface-stub "$P" upper
SIG=$(grep 'extern "C-unwind" fn' "$P/components/upper-host/src/lib.rs")
[[ -n "$SIG" ]] || { echo "FAIL: interface-stub produced no signature"; exit 1; }
echo "ok: generated signature — ${SIG#pub extern \"C-unwind\" fn }"

impl_with() { # body...
	{
		echo "use trantor_abi as abi;"
		echo "use abi::*;"
		echo "#[unsafe(no_mangle)]"   # without it the symbol is mangled and the
		                              # link fails on a missing host symbol
		echo "$SIG"          # the generated line, kept verbatim; it opens the fn
		cat                  # the body, as its own block
		echo "}"             # and the fn's close
	} > "$P/components/upper-host/src/lib.rs"
}

# ---- 5. the quick way: shell out --------------------------------------------
impl_with <<'RUST'
{
    let s = arg0.as_str().to_string();
    unsafe { arg0.decref(abi::host()); }
    let out = std::process::Command::new("tr").arg("a-z").arg("A-Z")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
        .spawn().and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(s.as_bytes())?;
            c.wait_with_output()
        }).expect("tr");
    print!("{}", String::from_utf8_lossy(&out.stdout));
    println!();
}
RUST
step "trantor run (shelling out)" "$TR" run "$P" -- ignored
SHELLED=$("$TR" run "$P" 2>/dev/null)
[[ "$SHELLED" == "RESIZE ME" ]] || { echo "FAIL: subprocess run gave '$SHELLED'"; exit 1; }
echo "ok: the work happens in a subprocess — '$SHELLED'"

# ---- 6. move it into a Rust library -----------------------------------------
step "cargo add" cargo add --path "$FIX/upper-lib" --manifest-path "$P/components/upper-host/Cargo.toml"
step "cargo metadata (rust-analyzer's precondition)" cargo metadata --format-version 1 --manifest-path "$P/Cargo.toml"
impl_with <<'RUST'
{
    let loud = upper_lib::shout(arg0.as_str());
    unsafe { arg0.decref(abi::host()); }
    println!("{loud}");
}
RUST
step "trantor run (in Rust)" "$TR" run "$P" -- ignored
NATIVE=$("$TR" run "$P" 2>/dev/null)

# ---- 7. what the walkthrough is actually for --------------------------------
[[ "$NATIVE" == "$SHELLED" ]] || { echo "FAIL: '$NATIVE' != '$SHELLED'"; exit 1; }
APP_AFTER=$(shasum "$P/app/main.roc" | cut -d' ' -f1)
[[ "$APP_AFTER" == "$APP_BEFORE" ]] || { echo "FAIL: the app changed when the implementation moved"; exit 1; }
grep -q 'upper_lib::shout' "$P/components/upper-host/src/lib.rs" \
	|| { echo "FAIL: positive control — the second run did not use the library at all"; exit 1; }
echo "ok: same output, same app source ($APP_BEFORE), work moved from a subprocess into a crate"

# ---- 8. and the generated signature survived both --------------------------
[[ "$(grep 'extern "C-unwind" fn' "$P/components/upper-host/src/lib.rs")" == "$SIG" ]] \
	|| { echo "FAIL: the implementations did not keep the generated signature"; exit 1; }
step "trantor check" "$TR" check "$P"
step "trantor test" "$TR" test "$P"
echo "U1 front door PASS"
