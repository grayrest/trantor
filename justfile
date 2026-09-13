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
    # Every fixture's COMPLETE output goes to a file, pass or fail. The console
    # still shows the last twelve lines of a failure, which is all it can
    # usefully hold — but twelve lines is not a diagnosis. An intermittent b8
    # failure was investigated for an afternoon on a tail that had already
    # scrolled past the cause, and a second occurrence told us nothing because
    # the evidence was gone the moment the runner returned.
    #
    # Written as the script runs rather than captured into a variable, so the
    # log survives a killed or hung run — which is the case most worth having
    # it for. target/ is gitignored, so this cannot dirty the tree.
    logs=target/verify-logs
    rm -rf "$logs"; mkdir -p "$logs"
    ran=0; failed=(); skipped=()
    for s in "${scripts[@]}"; do
        [[ -n "$filter" && "$s" != *"$filter"* ]] && continue
        ran=$((ran+1))
        name="$(dirname "${s#tests/golden/}")/$(basename "$s")"
        printf '%-46s' "$name"
        log="$logs/${name//\//__}.log"
        if bash "$s" > "$log" 2>&1; then
            # A fixture that could not run exits 0 printing SKIP. That is not a
            # pass, and printing PASS for it hid a missing sibling checkout.
            if grep -q '^SKIP' "$log"; then
                echo "SKIP"; skipped+=("$s"); sed -n 's/^SKIP/      | SKIP/p' "$log"
            else
                echo "PASS"
            fi
        else
            echo "FAIL"
            failed+=("$s")
            sed 's/^/      | /' "$log" | tail -12
            # Kept aside under a timestamp so the NEXT run cannot clobber the
            # one occurrence of a rare failure.
            keep="$logs/failed-$(date +%Y%m%dT%H%M%S)-${name//\//__}.log"
            cp "$log" "$keep"
            echo "      | full log: $keep"
        fi
    done
    [[ $ran -gt 0 ]] || { echo "verify: filter '$filter' matched no fixture"; exit 1; }
    dirty=$(git status --porcelain | wc -l | tr -d ' ')
    echo "----"
    echo "verify: $ran script(s) run, ${#failed[@]} failed, ${#skipped[@]} skipped, working tree $dirty file(s) dirty"
    echo "verify: full logs in $logs/"
    if [[ ${#failed[@]} -gt 0 ]]; then printf 'FAILED: %s\n' "${failed[@]}"; exit 1; fi
    [[ "$dirty" == 0 ]] || { echo "FAIL: the suite dirtied the working tree"; git status --short; exit 1; }
    # A skip checked nothing, so it is not a pass: a gate reading only the exit
    # status stayed green with no sibling checkout at all.
    if [[ ${#skipped[@]} -gt 0 ]]; then printf 'SKIPPED: %s\n' "${skipped[@]}"; echo "verify: not a full pass"; exit 1; fi
