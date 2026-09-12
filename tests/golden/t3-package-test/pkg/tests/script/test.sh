set -euo pipefail
[[ -x "$TRANTOR" && -n "$ROC" && -f "$PKG/package.toml" && -d "$TMP" ]]
grep -q '^greet = { path = ' <<<"$DEPS"
grep -q '^trantor-cli = { path = ' <<<"$DEPS"
echo "ok: the script got TRANTOR, ROC, PKG, DEPS and TMP"
