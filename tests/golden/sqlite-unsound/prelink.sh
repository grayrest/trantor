#!/usr/bin/env bash
# Pre-link hook run by `hematite build` after staging, before `roc build`.
#
# turso's iana_time_zone (via chrono, deep in turso_core) needs macOS
# CoreFoundation, which roc's linker only provides through a platform-bundled
# macos-sysroot. Symlink the host SDK's libSystem + CoreFoundation into one
# (generated, gitignored). Harmless for the rusqlite world (it references no
# framework, so the -framework flag is inert).
set -euo pipefail
cd "$(dirname "$0")"
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
