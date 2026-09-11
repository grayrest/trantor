# trantor dev recipes.

# Generate a self-signed localhost cert for TRANTOR_HTTP_EXTRA_CA (H13/H14).
# Writes the cert PEM to <cert-path> and the key to <cert-path>.key, and prints
# the `export TRANTOR_HTTP_EXTRA_CA=<cert-path>` line to wire it into the HTTP
# client. local-cert is a standalone crate (its crypto deps stay out of the
# trantor tool's build), so it's run by --manifest-path, not `-p`.
make-local-cert cert-path:
    @command -v cargo >/dev/null 2>&1 || { echo "make-local-cert: needs cargo on PATH (install Rust)"; exit 1; }
    cargo run --quiet --release --manifest-path tools/local-cert/Cargo.toml -- {{cert-path}}

# Run every golden fixture's acceptance script. There was no runner until U1
# P7: 19 verify.sh files and zero references to them here, so "the golden suite
# is green" meant a human running nineteen scripts and remembering the result.
# verify-tier.sh had in fact been red since a9ab498 (D-H7-38 moved dist/ and
# missed two paths) and nobody knew.
#
# `git status --porcelain` must be empty afterwards: the fixtures compose into
# target/ and a fixture that dirties the tree has either lost a .gitignore line
# or written generated output somewhere tracked.
verify *filter:
    #!/usr/bin/env bash
    set -uo pipefail
    cd "{{justfile_directory()}}"
    cargo build --release -q || exit 1
    mapfile -t scripts < <(ls tests/golden/*/verify*.sh | sort)
    [[ ${#scripts[@]} -gt 0 ]] || { echo "verify: found no fixture scripts — the glob is wrong"; exit 1; }
    filter="{{filter}}"
    ran=0; failed=()
    for s in "${scripts[@]}"; do
        [[ -n "$filter" && "$s" != *"$filter"* ]] && continue
        ran=$((ran+1))
        printf '%-46s' "$(dirname "${s#tests/golden/}")/$(basename "$s")"
        if out=$(bash "$s" 2>&1); then
            echo "PASS"
        else
            echo "FAIL"
            failed+=("$s")
            sed 's/^/      | /' <<<"$out" | tail -12
        fi
    done
    [[ $ran -gt 0 ]] || { echo "verify: filter '$filter' matched no fixture"; exit 1; }
    dirty=$(git status --porcelain | wc -l | tr -d ' ')
    echo "----"
    echo "verify: $ran script(s) run, ${#failed[@]} failed, working tree $dirty file(s) dirty"
    if [[ ${#failed[@]} -gt 0 ]]; then printf 'FAILED: %s\n' "${failed[@]}"; exit 1; fi
    [[ "$dirty" == 0 ]] || { echo "FAIL: the suite dirtied the working tree"; git status --short; exit 1; }
