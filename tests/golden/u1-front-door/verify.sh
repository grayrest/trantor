#!/usr/bin/env bash
# U1 acceptance: the whole front door, in the order a user meets it.
#
# The scenario is the one the plan was written around — do the work by shelling
# out first, then move it into a Rust library — with `tr` standing in for
# imagemagick and a local crate for the image crate, so the gate needs no
# network and no installed tools beyond the toolchain. Then, for work that needs
# no host at all, replace the Rust with a Roc component.
#
# The property under test is NOT that all three work. It is that the app does
# not know which one it got: the same interface, the same Roc source, byte for
# byte, across a subprocess implementation, a library one and a Roc one. If
# moving the work changes the app, the interface boundary did not hold and the
# walkthrough proved nothing.
set -euo pipefail
cd "$(dirname "$0")/../../.."
REPO=$PWD
FIX=$REPO/tests/golden/u1-front-door
# `pwd -P`, because mktemp hands back /var/folders/... and /var is a symlink to
# /private/var: `cargo add --path` counts its relative path from the resolved
# directory, and a harness path that disagrees with it is a harness bug.
# mktemp alone first: inside `cd "$(mktemp -d)"` a failed mktemp leaves `cd ""`,
# which macOS /bin/bash and Ubuntu's bash accept as staying put, so T
# became the repo root, which the EXIT trap then deleted.
T=$(mktemp -d); T=$(cd "$T" && pwd -P); P=$T/resize
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
# "Works the moment it is made" includes the app it wrote. The scaffold used to
# write `main!`'s signature from memory, which went on typechecking against this
# fixture's baseline long after trantor-cli's contract had moved on — so the
# untouched scaffold is checked here, before the next step replaces it.
step "trantor check (the untouched scaffold)" "$TR" check "$P"
# And tests. The scaffold's workspace has no members, which cargo refuses to
# test, so `trantor test` failed on a project before it had any Rust in it.
step "trantor test (the untouched scaffold)" "$TR" test "$P"

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
perl -pi -e 's/do_it!/shout!/; s/do_it/shout/' "$P/interfaces/upper/interface.toml"

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
# The crate sits beside the project, the way a user's own crate does, so cargo
# writes `path = "../../../upper-lib"`. That path is right from the component
# and wrong from its copy under target/trantor/<world>/components, two levels
# deeper — which is what `trantor run` builds. A crate further away (the repo
# fixture) is written with enough `..` to climb to / from both, and hid this.
cp -R "$FIX/upper-lib" "$T/upper-lib"
step "cargo add" cargo add --path "$T/upper-lib" --manifest-path "$P/components/upper-host/Cargo.toml"
grep -q 'path = "../../../upper-lib"' "$P/components/upper-host/Cargo.toml" \
	|| { echo "FAIL: positive control — cargo add did not write the short relative path"; exit 1; }
step "cargo metadata (rust-analyzer's precondition)" cargo metadata --format-version 1 --manifest-path "$P/Cargo.toml"
impl_with <<'RUST'
{
    let loud = upper_lib::shout(arg0.as_str());
    unsafe { arg0.decref(abi::host()); }
    println!("{loud}");
}
RUST
# From beside the project, named relatively, as `trantor new resize` taught.
# A relative world dir is what the user types, and the re-anchoring has to
# work from it: it once compared the relative path with nothing and gave up.
in_parent() { (cd "$T" && "$@"); }
step "trantor run (in Rust)" in_parent "$TR" run resize -- ignored
NATIVE=$(in_parent "$TR" run resize 2>/dev/null)

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

# ---- 9. the same interface, in Roc ------------------------------------------
# Uppercasing needs no host, so the last move takes the Rust out entirely. The
# baseline's `stdout` interface is what the Roc component prints through; the
# app cannot see it (the baseline exports nothing), so it cannot matter to the
# app which side does the printing.
#
# Measured before the switch, so the "not linked" check after it is not
# vacuously true of a binary that never had the symbol. Symbols go to a file
# first: `grep -q` quits at the first match, and under pipefail nm's SIGPIPE
# would fail a check that found what it looked for.
nm "$P/target/trantor/resize/bin/app" > "$T/syms.rust"
grep -q 'trantor__upper_host__shout' "$T/syms.rust" \
	|| { echo "FAIL: positive control — the Rust build did not link the host implementation"; exit 1; }
mkdir -p "$P/components/upper-roc"
cat > "$P/components/upper-roc/Upper.roc" <<'ROC'
import Stdout

Upper :: [].{
	shout! : Str => {}
	shout! = |words| Stdout.line!(Str.with_ascii_uppercased(words))
}
ROC
# The declaration swapped in place, and the wiring pointed at it. The host
# component goes with it rather than being left unwired: a leftover crate is
# what a reader would take for the implementation.
perl -0pi -e 's/\[components\.upper-host\]\nkind = "host"\nlang = "rust"\nexports = \["upper"\]\n/[components.upper-roc]\nkind = "roc"\nimports = ["stdout"]\nexports = ["upper"]\n/; s/^upper = "upper-host"$/upper = "upper-roc"/m' "$P/world.toml"
rm -r "$P/components/upper-host"
perl -pi -e 's|members = \["components/upper-host"\]|members = []|' "$P/Cargo.toml"
! grep -q 'upper-host' "$P/world.toml" "$P/Cargo.toml" \
	|| { echo "FAIL: the host component is still named in the project"; exit 1; }

step "trantor run (in Roc)" "$TR" run "$P" -- ignored
IN_ROC=$("$TR" run "$P" 2>/dev/null)
[[ "$IN_ROC" == "$SHELLED" ]] || { echo "FAIL: Roc run gave '$IN_ROC', not '$SHELLED'"; exit 1; }
[[ "$(shasum "$P/app/main.roc" | cut -d' ' -f1)" == "$APP_BEFORE" ]] \
	|| { echo "FAIL: the app changed when the implementation moved into Roc"; exit 1; }
# Positive control: the platform that ran carries this component's source, and
# the binary no longer carries the Rust one.
cmp -s "$P/components/upper-roc/Upper.roc" "$P/target/trantor/resize/platform/Upper.roc" \
	|| { echo "FAIL: the composed platform's Upper is not the Roc component"; exit 1; }
nm "$P/target/trantor/resize/bin/app" > "$T/syms.roc"
! grep -q 'trantor__upper_host__' "$T/syms.roc" \
	|| { echo "FAIL: the Roc build still links the host implementation"; exit 1; }
echo "ok: same output, same app source ($APP_BEFORE), work moved from a crate into Roc"
step "trantor check (in Roc)" "$TR" check "$P"
step "trantor test (in Roc)" "$TR" test "$P"
echo "U1 front door PASS"
