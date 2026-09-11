# B0 — the resource model (drop-balance)

Gate B0 of [`plans/2026-09-04-basic-cli-port.md`](../plans/2026-09-04-basic-cli-port.md);
implements P5. Spike: `spikes/b0-resource/` (`verify.sh`).

## Result ✅

`opens=3 closes=3 live=0`, exit code 3 (== closes). Every `roc_dealloc` the
driver received was a registry **HIT** at exactly `data − 8`, and the three
destructors ran in Roc's drop order. The **borrow test held**: counter#2 was
bumped, passed by value to a pure-Roc `peek` (a refcount bump = borrow), bumped
again, and closed **exactly once** with "bumped 2 times" — the borrow did not
close it and the second bump used it live. Refcounting *is* ownership (P5).

## The mechanism, as built

The glue's `RocBoxPayloadDecref` destructor fires **only** on host-side
`decref_box_with`/`free_box_with`. A Roc-side last-drop calls plain
`roc_dealloc` — no hook. Since the driver owns `roc_dealloc` (one of the six
runtime symbols), the mechanism is a **dealloc registry** in the generated
`trantor_abi::resource` module:

- `resource::new(value) -> RocBox` — `allocate_box(8, 8, false)` (a
  `Box(U64)`), payload = raw pointer to a boxed Rust value, registers
  `base = data − 8 → destructor`.
- generated driver `roc_dealloc(ptr, …)` — `resource::on_dealloc(ptr)` first
  (runs + unregisters if hit), then the default free.
- `resource::get(box) -> &mut T` borrow; `resource::release(box)` decrement,
  routing a final drop to the linker `roc_dealloc` (so host-side releases hit
  the same hook); `resource::with(box, |st| …)` = get + release in one step.
- IDL: `interface.toml` `[[resources]]`; the binding module spells it
  `Counter :: Box(U64)` — basic-cli's exact `FileReader :: Box(U64)` form.

## The bug that was the whole spike — R-B1 was not the problem

First run: `closes=0 live=3`, and the trace showed **zero** `roc_dealloc`
calls — not a pointer mismatch (R-B1, which would have shown `miss` lines) but
no frees at all. Cause: the glue's **owned-argument contract** — *"hosted
functions receive owned refcounted arguments."* `bump! : Counter => U64`
received an owned reference on every call and never released it, so each call
leaked one count and the box could never reach zero.

**Hard rule for the port (added to the plan):** every hosted fn that takes a
resource — stream, descriptor, socket, `ZonedDateTime` — **owns** that argument
and must `resource::release` it (or use `resource::with`). Missing it does not
crash; it silently leaks the handle forever, which is the worst failure mode.
This applies equally to `RocStr`/`RocList` args (the glue's existing rule) —
resources just make the leak an unclosed file instead of a stray allocation.

R-B1 itself (base-vs-data) is **resolved**: Roc passes the allocation base.

## Exit ✅

`spikes/b0-resource/verify.sh`: composes, builds, runs, asserts exit code 3 and
"closes=3 live=0" on stderr. trantor emits the `[[resources]]`-declared
`Box(U64)` aliases (author-written in the binding module, D13 verbatim) and the
generated driver's `roc_dealloc` consults the registry. B1 may build streams.
