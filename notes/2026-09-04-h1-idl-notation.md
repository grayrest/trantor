# H1 — IDL notation sketch (paper)

Gate H1 of [`plans/2026-09-04-trantor-v1.md`](../plans/2026-09-04-trantor-v1.md).
Paper only: this proves the superset (D3) can carry both real boundary shapes
and fixes the projectable-subset line. No code.

The IDL is WIT-shaped in its core type system and syntax, with named Roc
extensions. An interface using only the core **projects to a real `.wit` file**;
one using an extension is Roc-only and trantor *reports* it (D3, never errors).

## 1. Core type system — the projectable subset

Direct correspondence, both directions checked against seahaven's plain-data
boundary (the WIT-friendly platform):

| trantor IDL | WIT | Roc surface | glue (Rust) |
| --- | --- | --- | --- |
| `bool u8 u16 u32 u64 s8..s64 f32 f64` | same | `Bool U8 … I64 F32 F64` | scalars |
| `char` | `char` | `U32` (scalar value) | `u32` |
| `string` | `string` | `Str` | `RocStr` |
| `list<T>` | `list<T>` | `List(T)` | `RocList<T>` |
| `record { … }` | `record` | closed record | `#[repr(C)]` struct |
| `variant { a(T), b }` | `variant` | closed tag union | tag-after-payload enum |
| `enum { … }` | `enum` | closed no-payload tags | discriminant |
| `result<T, E>` | `result` | `Try(T, E)` | `RocResult` |
| `option<T>` | `option` | `[Some(T), None]` | tagged |
| `tuple<A, B>` | `tuple` | `(A, B)` | `#[repr(C)]` tuple |

Every seahaven crossing except the `U128` ones lands here. Example — the
primitive filesystem interface, in the projectable subset:

```
// interface roc:cli/filesystem@0.1.0    (projects to wasi-shaped .wit)
use roc:cli/io.{ io-err }

variant path-type { file, dir, sym-link, other }

file-read-bytes: func(p: native-path) -> result<list<u8>, io-err>
path-type:       func(p: native-path) -> result<path-type, io-err>
env-var:         func(name: native-os-str)
                   -> result<native-os-str, var-not-found-or-env-err>
```

`io-err` is `use`d from one definition site (`roc:cli/io`), not redeclared —
the nominal-identity requirement from D7/D16, which the projectable subset must
honor or two interfaces get two incompatible `IOErr`s.

## 2. The two `U128` gaps — DECISION

`utc_now! : () => Try(U128, …)` and the three `file_time_*` return `U128`; WIT
tops out at `u64`. Two options were on the table (`tuple<u64,u64>` lowering vs
superset-only). **Decision: lower `u128`/`s128` to `tuple<u64, u64>` (hi, lo) in
the projection, and keep `u128` as a first-class core type in the IDL.**

Rationale: the value is a real timestamp/duration a WASI consumer will want, not
an exotic type; `tuple<u64,u64>` is lossless and unambiguous; and it keeps these
four seahaven symbols *inside* the projectable subset instead of exiling the
whole clock/filesystem-time surface to Roc-only. The projection annotates the
lowering in a comment so a WIT consumer reassembles it. This is a projection
rule, not an extension — the IDL type stays `u128`.

## 3. The extensions — what the subset cannot express

roc-solid's boundary needs three things WIT has no vocabulary for. Named
extensions, each of which makes an interface Roc-only (reported):

### `box<T>` — a Roc-refcounted heap value crossing by reference

```
box<T>          // extension: a RocBox<T>, refcounted, shared-everything
```

### `closure` — a Roc function value crossing the boundary, with ownership

WIT has no function values. The extension carries the call direction and the
refcount-ownership of the capture, because that is the load-bearing fact glue
needs and the thing that silently corrupts memory if wrong (D3a):

```
closure<(Args) => Ret> { ownership }
   ownership ∈ { donated, borrowed }
```

`donated` = one reference transfers to the callee (roc-solid's `dep_thunk!`
"donated reference"); `borrowed` = caller retains, callee must not release.

### The two hard crossings, expressed

roc-solid `Host.roc` line 15 and 34, verbatim shapes:

```
// dep_thunk! : U64 => Box(({} => a))
dep-thunk: func(slot: u64)
   -> box<closure<() => T> { donated }>          // T is the slot's payload type

// register_binding! : List(U64), Box(({} => Str)) => U64
register-binding: func(deps: list<u64>,
                       render: box<closure<() => string> { donated }>)
   -> u64
```

Both are expressible. `signal_create!` (three boxed closures of distinct
signatures in one call) and `register_action!` (`Box((U64 => {}))`) fall out the
same way. **So the superset carries the entire roc-solid boundary** — no shape
was found that it cannot express. H1's NO-GO (a construct the superset can't
reach) did not fire.

## 4. D3a — the well-formedness rule, not a comment

G1 established that a funnel-crossing closure with **inferred open-row or unbound
dep types** typechecks, builds, then *silently corrupts memory when fired*
(mismatched monomorphization layouts under the erased downcast). So the `closure`
extension has a well-formedness condition trantor checks at compose time:

> Every type parameter of a `closure`'s argument and return must be a **closed,
> concrete** type at the interface definition — no open row (`[… , ..]`), no
> unbound variable. A `closure<() => T>` is only well-formed once `T` is pinned
> by the interface (e.g. `dep-thunk`'s `T` is the slot's declared payload type).

An interface that leaves a closure's types open is **rejected by trantor**, not
passed to `roc build` — because the failure mode is silent corruption, the one
thing that must never reach the compiler. This is the single place the IDL is
*stricter* than Roc itself, and deliberately so.

## Net for the plan

- Superset expresses both hard crossings and the whole seahaven boundary. ✅
- Projectable subset is precisely: core types + the `u128 → tuple<u64,u64>`
  projection rule. Everything using `box`/`closure` is Roc-only, reported.
- D3a is now a checkable well-formedness rule on the `closure` extension.
- H1 exit met: notation sketch covers both crossings and encodes D3a. No NO-GO.
- Feeds H3's manifest parser: the `.wit`-shaped interface grammar is the core
  table above plus `box<T>` and `closure<…>{ownership}` as the two extension
  productions.
