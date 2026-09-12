set -euo pipefail
[[ -x "$TRANTOR" && -n "$ROC" && -f "$PKG/package.toml" && -d "$TMP" ]]
grep -q '^greet = { path = ' <<<"$DEPS"
grep -q '^trantor-cli = { path = ' <<<"$DEPS"
grep -q '^trantor-cli = { path = ' <<<"$DEV_DEPS"
! grep -q '^greet = ' <<<"$DEV_DEPS"
echo "ok: the script got TRANTOR, ROC, PKG, DEPS, DEV_DEPS and TMP"
