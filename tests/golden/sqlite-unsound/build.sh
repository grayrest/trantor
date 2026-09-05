#!/usr/bin/env bash
# Build the roc:sqlite-unsound fixture from composed sources. `hematite compose`
# must have run first. Usage: build.sh <app-dir> <out-name>.
set -euo pipefail
cd "$(dirname "$0")"
APP="${1:-app}"; OUT="${2:-sq1}"
ROC="${ROC:-$HOME/.bin/roc}"; GLUE="${GLUE:-$HOME/.bin/RustGlue.roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }
mkdir -p glue-out; cap "$ROC" glue "$GLUE" glue-out platform/main.roc; cp glue-out/roc_platform_abi.rs abi/src/generated.rs
cargo build --release; mkdir -p platform/targets/arm64mac
# Stage ONLY the archives the composed platform actually links (main.roc inputs),
# so a stale archive from a prior world's build (e.g. libturso_host.a) can't
# leak into this world's targets.
rm -f platform/targets/arm64mac/lib*.a
for a in $(grep -oE 'lib[a-z0-9_]+\.a' platform/main.roc | sort -u); do cp "target/release/$a" "platform/targets/arm64mac/$a"; done

# turso's iana_time_zone (via chrono, deep in turso_core) needs macOS
# CoreFoundation, which roc's linker only provides through a platform-bundled
# macos-sysroot. Symlink the host SDK's libSystem + CoreFoundation into one
# (generated, gitignored). Harmless for the rusqlite world (it references no
# framework, so the -framework flag is inert).
SDK="$(xcrun --show-sdk-path 2>/dev/null || true)"
if [ -n "$SDK" ]; then
  SR=platform/targets/macos-sysroot
  # `usr` can be a symlink (accessed by path). The .framework must be a REAL
  # directory — roc's framework discovery skips symlinked entries — with the
  # .tbd symlinked inside it.
  mkdir -p "$SR/System/Library/Frameworks/CoreFoundation.framework"
  ln -sfn "$SDK/usr" "$SR/usr"
  ln -sfn "$SDK/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation.tbd" \
          "$SR/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation.tbd"
fi

cap "$ROC" check "$APP/main.roc"; mkdir -p bin; cap "$ROC" build --output="bin/$OUT" "$APP/main.roc"; echo "built bin/$OUT"
