# Upstream builtin gaps (recorded, not pursued)

Date: 2026-09-14. Compiler: roc `10e922df83`, function names read from
`~/Repositories/roc/generated-docs`.

These are gaps in Roc's builtin types found by the standard library survey
([`2026-09-14-stdlib-survey.md`](2026-09-14-stdlib-survey.md)). Under D-S1-1 they
belong upstream as methods, not in trantor. Under D-S1-13 the trantor effort
does not pursue them: no PRs, no fork patches, no stopgap modules. This file
exists so the list is not rediscovered.

Source tags: **R** Rust, **P** Python, **E** Elixir/OTP, **C** Clojure.

## `F64` / `F32` / `Dec`

Present: `abs`, `sqrt`, `pow`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
`is_nan`, `is_infinite`, `is_finite`, `floor_to_*_try`, `ceiling_to_*_try`,
`round_to_*_try`, `to_bits`/`from_bits`, `pi`, `e`, `tau`.

| Missing | From |
|---|---|
| `exp`, `exp_m1`, `ln`, `ln_1p`, `log2`, `log10`, `log(base)` | R P E C |
| `atan2`, `hypot`, `cbrt`, `sinh`/`cosh`/`tanh` | R P E C |
| Float-valued `floor`, `ceil`, `trunc`, `round`, `round_ties_even`, `fract` | R P C |
| `copysign`, `signum`, `mul_add` | R P C |
| Approximate equality (`is_close` with relative and absolute tolerance) | P |
| `total_cmp` (NaN-safe ordering) | R |
| Round to N decimal places | E P |

## Integers

Present: checked, wrapping, saturating and overflowing arithmetic; bitwise
ops; `count_leading_zero_bits`, `count_trailing_zero_bits`, `count_one_bits`;
`pow`, `pow_try`; `from_str`; `from_le_bytes`, `append_le_bytes_to`;
`div_ceil_by`, `is_multiple_of`, `abs_diff`.

| Missing | From |
|---|---|
| `from_str_radix`, `to_str_radix` | R P E |
| `from_be_bytes`, `append_be_bytes_to`, `swap_bytes` | R |
| `gcd`, `lcm` | P E |
| `isqrt`, `ilog2`, `ilog10` | R P |
| `rem_euclid`, `div_euclid` | R |
| `digits` (base-N digit list) | E |
| `rotate_left`, `rotate_right` bit rotation | R |
| A conversion from `U64` a generic function can call (`from_u64_wrap` on every integer type) | trantor-random, D-S4-8 |

## `Str`

Present: `len`, `is_empty`, `concat`, `contains`, `trim`/`trim_start`/
`trim_end`, `starts_with`, `ends_with`, `repeat`, `with_prefix`, `drop_prefix`,
`drop_suffix`, `replace_each`/`replace_first`/`replace_last`, `split_on`,
`split_first`, `split_last`, `join_with`, `iter_utf8`, `to_utf8`, `from_utf8`,
`from_utf8_lossy`, ASCII case conversion and caseless compare, `inspect`.

| Missing | From |
|---|---|
| Code point iteration (`iter_scalars`, `to_scalars`, `from_scalars`) | R E |
| `find`, `rfind`, `find_all` returning byte offsets | R P E |
| `split_n`, `rsplit_n` | R P |
| `lines` (handles `\r\n`), `split_whitespace` | R P C |
| `strip_prefix`/`strip_suffix` returning `Try` (not silently unchanged) | R |
| `char_indices`-style iteration with offsets | R |
| `is_char_boundary`, byte-offset `sublist` | R |
| `reverse` | C |
| `trim_matches` with a predicate or byte set | R |

## `List`

Present: `map`/`map2`–`map4`, `map_with_index`, `keep_if`/`drop_if`, `fold`,
`fold_rev`, `fold_until`, `fold_try`, `find_first`/`find_last` and indexes,
`any`, `all`, `count_if`, `contains`, `sublist`, `take_first`/`take_last`,
`drop_first`/`drop_last`, `split_at`, `split_on`, `split_if`, `chunks_of`,
`intersperse`, `join`, `join_map`, `sort`/`sort_by`/`sort_with` and reversed
variants, `min`, `max`, `sum`, `rev`, `swap`, `repeat`, `starts_with`,
`ends_with`.

| Missing | From |
|---|---|
| `windows(n)` | R C |
| `dedup`, `dedup_by` (adjacent); `distinct` (all) | R C |
| `group_by` into `Dict`, `frequencies` | E C |
| `partition` (by predicate, both halves) | E C |
| `chunk_by` / `partition_by` (split where a key changes) | E C |
| `take_while`, `drop_while`, `split_with` | R E C |
| `zip`, `unzip` (as tuples) | R P E C |
| `min_by`, `max_by` (key function) | R P E C |
| `scan` / `reductions` | R C |
| `binary_search`, `binary_search_by`, `partition_point` | R P |
| `rotate_left`, `rotate_right` | R |
| `interleave` | C |
| `flat_map` spelled as such (`join_map` exists) | R E C |
| `sort_by_cached_key` | R |

## `Dict` / `Set`

Present: `insert`, `insert_all`, `remove`, `remove_all`, `get`, `contains`,
`update`, `keep_if`, `drop_if`, `keep_shared`, `map`, `join_map`, `fold`,
`fold_until`, `keys`, `values`, `to_list`, `from_list`; `Set` `union`,
`intersection`, `difference`.

| Missing | From |
|---|---|
| `merge_with` (collision function) | E C |
| `get_or` (default value) | E |
| `update_keys`, `map_keys` | C |
| `symmetric_difference`, `is_subset`, `is_disjoint` | R P |
| `remove_if_present` returning the removed value | R |

## Compiler behaviour found while building packages

Not builtin methods, and not pursued either (D-S1-13). Recorded so the next
package does not rediscover them. Found on roc `10e922df83` building
trantor-random (D-S4).

- **A missing method inside a generic body is not a compile error.** In a
  function with a `where` clause, a call to a method no type has
  (`U64.to_le_bytes`, `U64.div_trunc`) prints a "missing method" report, but
  `roc test` counts "0 compiler errors" and the expect fails at runtime with
  "runtime error". Read the reports, not only the counts.
- **A nominal with a format-generic `parser_for` does not decode as a record
  field through `Json.parse`.** `Json.parse` of the bare value works; of
  `{ t : Tagged }` the field parser's error type is unified with `[]` and
  typechecking fails. Reproduced on a minimal nominal (`Tagged`), so it is not
  specific to `Uuid`. Encoding the same record works.
- **`Json.to_str` and `Json.parse` of the same nominal in one expect** fail the
  same way; separate expects pass.
- **A nominal record cannot be destructured in a parameter** (`|{ seed, buf }|`
  for `Rng :: { seed, buf, .. }`): use field access. A generator refilled
  through field access still updates its buffer in place (the D-S4-12 gate
  counted zero allocations).
- **A parameter or local named like a method of the enclosing type is a
  duplicate definition**, e.g. `|bytes|` inside a type that has a `bytes`
  method, or `rng` inside `UuidV7`, which has `rng`.
