# H6 — per-app worlds

Gate H6 of [`plans/2026-09-04-trantor-v1.md`](../plans/2026-09-04-trantor-v1.md).
Acceptance: `tests/golden/two-component/verify-tier.sh`.

## The product promise, mechanized

"Baseline consumed by URL; a straightforward path to extend." H6 turns that into
three tool operations over the H2 baseline:

- **`trantor publish <dir>`** → `dist/`: the platform's `.roc` sources, the
  prebuilt per-target archives, and `baseline.lock` carrying an **ABI
  fingerprint** (D11/H11). This is the shippable tarball an app depends on.
- **`trantor tier <dir> --world <ext>`** → classifies an extension:
  - pure-Roc additions → **Tier 1**: "reuses the baseline's prebuilt archives,
    no Rust toolchain."
  - any host component → **Tier 2**: "new hosted symbols, so full source
    composition (cargo + roc glue) is required. This crosses the tier cliff."

  Verified on two extension manifests (`extensions/tier1.toml` adds a `kind=roc`
  component; `extensions/tier2.toml` adds a `kind=host` one).

## Tier-1 build from the published baseline ✅

The test copies `dist/` to a temp "consumer" (simulating a download), injects a
pure-Roc `Greet` module + the two `main.roc` edits (exposes + import, per H4c),
and builds an app with **only `roc build`** — no cargo, no glue. It links
against the published prebuilt archives and prints `~ per-app world ~`.

## The ABI fingerprint, and the bug it taught

First cut hashed the compiler + glue **+ every platform `.roc` source**. That
was wrong: adding `Greet.roc` (a Tier-1 change that adds no hosted symbols)
changed a platform source and so flipped the fingerprint — which would falsely
reject every Tier-1 extension, defeating the whole point.

**Fix:** the fingerprint covers only what determines `libhost`'s ABI — the
compiler binary's identity and the glue spec. The baseline's hosted surface is
fixed in the published `main.roc`, and a Tier-1 add touches no hosted symbols, so
the prebuilt archives stay valid exactly when the consumer's compiler+glue match
the baseline's. Verified: the fingerprint is **stable across a Tier-1 addition**
(`63eee967…` before and after adding `Greet`), and a tampered/changed compiler
id flips it — the gate that stops a stale-glue mismatch from becoming a runtime
segfault (H11).

This mirrors the split in the upstream glue redesign: the real `roc_abi_assert!`
fingerprint enforces the same thing at *link* time; trantor's lock is the
*distribution*-time check the tarball carries so a Tier-1 consumer can decide
whether reuse is safe **without running glue**.

## Relationship to H4c

H4c proved the Tier-1 *mechanism* (pure-Roc add, archives reused, no toolchain)
against the working tree. H6 proves the *workflow*: a published, fingerprinted
baseline consumed as an artifact, the tier reported to the author, and the same
build succeeding from the published `dist/`. Together they are D10 + D11 end to
end.

## Exit ✅

`verify-tier.sh` green: publish with fingerprint; Tier 1 vs Tier 2 classified
correctly; a Tier-1 extension built from the published baseline with the Rust
toolchain unused; fingerprint stable under Tier-1 and sensitive to a
compiler change.
