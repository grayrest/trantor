# S3 — trantor-encoding: design log (2026-09-16)

**Plan:** [`plans/2026-09-16-s3-trantor-encoding.md`](../plans/2026-09-16-s3-trantor-encoding.md).
**Roadmap:** D-S1-11 step 3 in
[`2026-09-14-s1-stdlib-roadmap-design-log.md`](2026-09-14-s1-stdlib-roadmap-design-log.md)
(Base64 and hex, CSV, TOML; CSV moves in per D-S1-12).

Settled against compiler `release-fast-10e922df`, `~/dev/roc/playground/csv`
(not a git repository), trantor-hash and trantor-temporal as of this date.

## What was found before any question was asked

- **No TOML or Base64 in Roc anywhere under `~/dev/roc`.** trantor's Rust uses
  the `toml` and `toml_edit` crates; nothing on the Roc side parses either.
  Hex exists only for the builtin `Crypto` digests.
- **The playground CSV package** is about 2,000 lines of pure Roc in eight
  public modules (`Csv`, `Tsv`, `Table`, `Row`, `Dialect`, `Error`, `Decode`,
  `Encode`), with typed decode and encode as `Encoding` formats and 156
  assertions in `Test.roc` plus a depth gate in `Stress.roc`. All 156 pass on
  `10e922df`. `~/dev/roc/playground/Test.roc`, a basic-cli app, imports it.
- **A format is a nominal type with `encode_*`/`parse_*` methods;** the
  compiler derives `encoder_for`/`parser_for` per shape and a `where` clause
  demands them (`Builtin.roc`'s `Json` is the reference). Methods are matched
  by name only.
- **Measured on `10e922df`:**
  - A format type nested in a module (`Fmt :: [].{ Enc := … }`), with its
    entry point on the outer module, encodes a record: `roc test` passes.
  - A record with a `Bool` field now reaches `encode_bool`. The playground's
    "Bool is a compile error" gap is closed.
  - A nominal type whose hand-written `encoder_for` requires a method only one
    format defines (`encode_day`) encodes inside a record through that format.
    The parse side was not measured.
- **trantor-hash is the host-less precedent:** one `kind = "roc"` component,
  `trantor-cli` only in `[dev-deps]`, internal modules exported by the
  component but not the package.
- **trantor-temporal:** `PlainDate := { year : I32, month : U8, day : U8, cal :
  Calendar ?? Iso }` and `PlainTime := { hour, minute, second : U8, millisecond,
  microsecond, nanosecond : U16 }` are pure nominal records; `ZonedDateTime` is
  a host resource made only by effects; there is no `PlainDateTime` and no
  instant type. No temporal type defines `encoder_for`/`parser_for`.
  trantor-temporal has host components, so trantor-encoding cannot depend on
  it without losing D-S1-7's host-free property.
- **TOML 1.1.0** was released 2025-12-18: multi-line inline tables with
  trailing commas, `\xHH` and `\e` escapes, optional seconds, and
  clarifications (comments never affect output; any integer or float size
  permitted). A 1.1 parser reads every 1.0 document.

## Decisions

### D-S3-1 One public module per format

`Base64`, `Hex`, `Csv`, `Toml`. Each holds its own types, format type and entry
points: CSV's `Table`, `Row`, `Dialect` and error become `Csv.Table`,
`Csv.Row`, `Csv.Dialect`, `Csv.Err`. (User.)

**Why:** the playground's `Decode`, `Encode` and `Error` collide with TOML's in
one package, and `Table`/`Row` do not say which format they belong to. Matches
`Hash`, `Temporal`, `Glob`; nesting the format type was measured to work.

**Rejected:** the playground's split with prefixes (`CsvDecode`, `TomlEncode`,
…) — less restructuring, many more imports.

### D-S3-2 Base64: four functions

*(Length and index details: D-S3-37. Edge cases and the full error row: D-S3-54.5.)*

```roc
Base64.encode : List(U8) -> Str                                 # standard, padded
Base64.encode_url : List(U8) -> Str                             # URL-safe, unpadded
Base64.decode : Str -> Try(List(U8), [InvalidBase64(U64)])      # padding optional
Base64.decode_url : Str -> Try(List(U8), [InvalidBase64(U64)])  # padding optional
```

The index is the first bad character's. Nonzero trailing bits are rejected, so
each byte list has one valid encoding. (User.)

**Rejected:** an options record (`{ alphabet, padding }`) on every call; MIME
line wrapping; Base32.

### D-S3-3 Hex: two functions

```roc
Hex.encode : List(U8) -> Str                                    # lowercase
Hex.decode : Str -> Try(List(U8), [InvalidHex(U64), OddLength])  # either case
```

(User.)

**Rejected:** `encode_upper`; separators, `0x` prefixes and whitespace on
decode.

### D-S3-4 `Csv` keeps all three layers, renamed

*(Errors, decode and encode signatures: D-S3-39. Header union, duplicate headers, cell grammar: D-S3-45, D-S3-49, D-S3-50.)*

```roc
Csv.parse : Str -> Try(List(List(Str)), Csv.Err)
Csv.parse_with : Str, Csv.Dialect -> Try(List(List(Str)), Csv.Err)
Csv.to_str : List(List(Str)) -> Str
Csv.to_str_with : List(List(Str)), Csv.Dialect -> Str
Csv.table : Str -> Try(Csv.Table, Csv.Err)
Csv.table_with : Str, Csv.Dialect -> Try(Csv.Table, Csv.Err)
Csv.decode : Str -> Try(List(a), [Scan(Csv.Err), BadCells(List(Csv.Bad)), ..]) where [...]
Csv.decode_with : Str, Csv.Dialect -> ...
Csv.encode : List(a) -> Str where [...]
Csv.encode_with : List(a), Csv.Dialect -> Str where [...]
Csv.Dialect.csv, Csv.Dialect.tsv
```

`Table` and `Row` methods are unchanged; `parse_with_headers` becomes `table`;
`Tsv` becomes `Csv.Dialect.tsv`. `parse`/`to_str` are text to rows,
`decode`/`encode` are typed, as the builtin `Json` splits them; `_with` takes a
dialect. `Bool` fields encode as `true`/`false` (the compiler gap closed).
(User.)

**Rejected:** dropping `Table`/`Row` — a tool reading whatever columns a file
has cannot declare a record.

### D-S3-5 `Csv.encode_columns` fixes column order

*(`encode_columns` also answers `Encode(Csv.EncodeErr)`: D-S3-39. `UnknownColumn` for names in no record, `DuplicateColumn`: D-S3-45, D-S3-49.)*

```roc
Csv.encode_columns : List(a), List(Str) -> Try(Str, [UnknownColumn(Str), MissingColumn(Str)]) where [...]
Csv.encode_columns_with : List(a), List(Str), Csv.Dialect -> Try(Str, [UnknownColumn(Str), MissingColumn(Str)]) where [...]
```

`encode` stays alphabetical, the derive's order. A record field left out of the
list is `MissingColumn`, so data is not dropped by accident. (User.)

**Rejected:** alphabetical only; column order as a `Dialect` field (a dialect
is syntax, not data).

### D-S3-6 `Toml` reads, writes and edits in place, all in this step

(User: "edit now".) The driver is a Roc tool that edits a hand-written file the
way `trantor add` edits `world.toml` through `toml_edit`.

**Rejected:** read and write now, editing later.

### D-S3-7 TOML 1.1 read; 1.0 subset written by default, 1.1 as a variant

(User.) Reading accepts 1.1.0, so 1.0 documents too. Writing defaults to what a
1.0 parser accepts; D-S3-16 defines the variant.

**Rejected:** 1.0 only, which rejects files written today; writing 1.1 by
default, which breaks 1.0 readers.

### D-S3-8 The untyped value tree

*(Nominal, with its own equality and codecs: D-S3-32, D-S3-33. Depth: D-S3-34. Floats are `Float(Toml.Float)`: D-S3-38.)*

```roc
Toml.Value : [
    String(Str), Integer(I64), Float(F64), Boolean(Bool),
    OffsetDatetime({ date : Toml.Date, time : Toml.Time, offset : Toml.Offset }),
    LocalDatetime({ date : Toml.Date, time : Toml.Time }),
    LocalDate(Toml.Date),
    LocalTime(Toml.Time),
    Array(List(Toml.Value)),
    Table(List((Str, Toml.Value))),      # document order, keys unique
]
```

Integers outside `I64` are a parse error, as the spec requires. `Z` and
`+00:00` both parse to offset 0; `Document` keeps the spelling. (User; the
`Date`/`Time`/`Offset` shapes are D-S3-23's.)

**Why ordered lists for tables:** deterministic output in file order; `Dict`
iteration order is unspecified. Lookups are linear, fine for config files.

**Rejected:** `Dict(Str, Value)`; dates and times as `Str`.

### D-S3-9 Typed TOML dates and times are TOML-only nominal types

*(Integers into float fields: D-S3-37.)*

```roc
Toml.OffsetDatetime := { date : Toml.Date, time : Toml.Time, offset : Toml.Offset }
Toml.LocalDatetime := { date : Toml.Date, time : Toml.Time }
Toml.LocalDate := Toml.Date
Toml.LocalTime := Toml.Time
```

Their `encoder_for`/`parser_for` require the date methods of D-S3-22, which
only date-capable formats define, so using them with `Json` is a compile error.
A `Str` field does not accept a TOML date. Integers decode into any width that
fits, floats into `F32`/`F64`/`Dec`, sub-tables into records, arrays into
`List`; a missing key is `MissingRequiredField(name)` unless the field has a
default.
(User.)

**Rejected:** letting `Str` fields accept dates in RFC 3339 — the type
information is lost.

### D-S3-10 Editing: key-path operations on an opaque `Document`

*(Paths, errors, removal, table style and `append`: D-S3-29..31. Kind changes: D-S3-47. Section placement: D-S3-53.)*

```roc
Toml.Document :: ...
Toml.parse_document : Str -> Try(Toml.Document, Toml.Err)
to_str : Document -> Str                      # unedited: byte-identical to the input
to_value : Document -> Toml.Value
get : Document, List(Str) -> Try(Toml.Value, [NotFound])
set : Document, List(Str), Toml.Value -> Try(Document, [NotATable(List(Str)), Encode(Toml.EncodeErr)])
set_with : Document, List(Str), Toml.Value, Toml.Write -> Try(Document, [NotATable(List(Str)), Encode(Toml.EncodeErr)])
remove : Document, List(Str) -> Try(Document, [NotFound])
append : Document, List(Str), Toml.Value -> Try(Document, [NotFound, NotAnArray])
```

- Comments, blank lines, key order, quoting, number spelling and table style
  (`[header]`, dotted, inline) are kept.
- `set` on an existing key replaces only the value; the key, surrounding
  whitespace and a trailing comment stay.
- `set` on a new key places it after the table's last key with its neighbours'
  indentation. Missing parent tables are created as `[header]` sections at the
  end, or as dotted keys where the parent is already dotted.
- `remove` takes the line and the comment lines directly above it.
- No comment-editing API; arrays only append or are replaced whole.

(User.)

**Rejected:** a cursor or mutable-handle API (`toml_edit`'s shape) that does
not suit immutable values; edits as text patches, which chain badly.

### D-S3-11 Parse errors: three kinds, first error only

*(Keys are segments: D-S3-30. Second 60 and depth: D-S3-34, D-S3-37. Multi-line string line breaks: D-S3-43. Duplicate keys are segments: D-S3-44.8.)*

```roc
Toml.Err : [
    Syntax({ line : U64, column : U64, expected : Str }),
    Duplicate({ line : U64, column : U64, key : List(Str) }),
    OutOfRange({ line : U64, column : U64, text : Str }),
]
Toml.decode : Str -> Try(a, [Parse(Toml.Err), Mismatch({ key : List(Str), expected : Str }), MissingRequiredField(Str), ..]) where [...]
Toml.err_to_str : Toml.Err -> Str
```

Lines are 1-based, columns in code points. A duplicate points at the second
definition. CSV keeps its own error type. (User.) *(`MissingRequiredField`
carries `Str`, not a path: D-S3-28.)*

**Rejected:** collecting every syntax error in one pass — error recovery for an
editor use nobody has.

### D-S3-12 `Base64.Bytes` and `Hex.Bytes` *(withdrawn by D-S3-27)*

```roc
Base64.Bytes := List(U8)      # standard padded Base64, through encode_str/parse_str
Hex.Bytes := List(U8)         # lowercase hex, through encode_str/parse_str
```

They work in any format with strings (JSON, TOML, CSV). `bytes` and
`from_bytes` convert. A bad string is a `parse_str` failure with a tag naming
the type. `Base64.UrlBytes` waits for a use. (User.)

**Rejected:** no types, every binary field converting by hand.

### D-S3-13 Package layout

*(A fourth component, `datetime`: D-S3-52.)*

`~/dev/roc/trantor-encoding`: package exports `Base64`, `Hex`, `Csv`, `Toml`;
components `base64` (`Base64`, `Hex`), `csv` and `toml`, each `kind = "roc"`,
internal modules prefixed by format (`CsvParse`, `TomlLex`, …) and exported by
the component only; `trantor-cli` in `[dev-deps]` only. (User.)

**Why components per format family:** a world can replace one format's
implementation and keep the others, as trantor-terminal's layers are replaced.

**Rejected:** one component for everything.

### D-S3-14 Tests

*(Conformance lists and comparison: D-S3-35. Added cases: D-S3-37. Round 2 test additions and the 1.0.0 lists: D-S3-44. CSV assertions rewritten for D-S3-39, one depth per D-S3-34: D-S3-54.14. Round 3 tests: D-S3-54.15.)*

| Type | What |
|---|---|
| Unit | RFC 4648 §10 vectors and rejections; the 156 CSV assertions renamed, plus `encode_columns` and `Bool`; TOML lexer, values, each error kind, typed decode of every scalar width and the date types |
| Conformance | `toml-lang/toml-test` checked in and pinned (commit in a README): valid cases match their JSON, invalid cases are rejected, for 1.0 and 1.1; an app suite, since `expect`s cannot read files |
| Round-trip | every valid corpus file through `parse_document`/`to_str` byte-identical; `decode(encode(v)) == v` in both write modes |
| Snapshot | each D-S3-10 placement rule as before, edit, after |
| Stress | the CSV depth gate; TOML nesting at the D-S3-19 limit and a 10,000-key document |

(User.)

**Rejected:** `toml_edit` as an oracle for edit output (its formatting is not a
spec); a submodule for the corpus; security and performance benchmarks.

### D-S3-15 How `Toml.encode` formats a new document

*(Dict keys sorted: D-S3-32. Escapes and floats pinned: D-S3-37. Fraction digits, empty keys: D-S3-44. Kept float spellings: D-S3-38, D-S3-54.10.)*

Scalars and arrays first, then sub-tables as `[section]`, then arrays of tables
as `[[section]]`; tables inside arrays inline. Basic strings with escapes,
multi-line basic strings when the value has newlines. Bare keys when
`A-Za-z0-9_-`, quoted otherwise. Decimal integers without underscores; the
shortest float spelling that reads back identically (`1.0`, `inf`, `nan`).
RFC 3339 dates with `Z` for offset 0. Arrays on one line. A blank line between
sections and a final newline. (User.)

**Rejected:** inline tables everywhere; width-based array wrapping.

### D-S3-16 The TOML entry points and the 1.1 write variant

*(What V1_1 changes, and `set` not detecting 1.1: D-S3-37. `set_with` takes `Toml.Edit`: D-S3-29.)*

```roc
Toml.parse : Str -> Try(Toml.Value, Toml.Err)
Toml.to_str : Toml.Value -> Try(Str, Toml.EncodeErr)
Toml.to_str_with : Toml.Value, Toml.Write -> Try(Str, Toml.EncodeErr)
Toml.decode : Str -> Try(a, [...]) where [...]
Toml.encode : a -> Try(Str, Toml.EncodeErr) where [...]
Toml.encode_with : a, Toml.Write -> Try(Str, Toml.EncodeErr) where [...]
Toml.Write : { version : [V1_0, V1_1] }
```

`V1_1` writes `\e` and `\xHH` for control characters up to U+00FF, omits
seconds when seconds and fractions are zero, and writes inline tables inside
arrays one per line with a trailing comma. `Document.set` writes 1.1 when the
document already contains 1.1 syntax, and `set_with` chooses. No
`decode_with`: reading always accepts 1.1. (User; return types amended by
D-S3-24.)

**Rejected:** separate `_v1_1` functions, which double with every option.

### D-S3-17 Encoded output is byte-stable within a major version

*(`expected` and `err_to_str` text are message text: D-S3-54.11.)*

`Csv.encode`/`to_str`, `Toml.encode`/`to_str` in both modes, and the text
`Document` inserts are byte-identical for the same input within a major
version, pinned by golden `expect`s. Error message text is not covered; error
tags are. Base64 and hex are fixed by the RFC. (User.) D-S1-6 for text.

**Rejected:** no promise; a round-trip-only promise, which lets checked-in
generated files change on upgrade.

### D-S3-18 The playground is left alone

`~/dev/roc/playground` is not touched: no deletion and no note. The code is
copied into trantor-encoding. (User: "leave it".)

**Why:** `playground/Test.roc` runs on upstream basic-cli and imports the
playground copy, and the playground has no history.

**Rejected:** deleting the copy after the move (D-S3-13 as first proposed);
a frozen-copy note; converting `Test.roc` to a trantor project.

### D-S3-19 Nesting is capped at 128 levels

*(Superseded by D-S3-34: one depth over every kind of nesting.)*

Deeper arrays or inline tables are `Toml.Err.OutOfRange` with the position, in
`parse`, `decode` and `parse_document`. Table headers nest without recursion
and are not capped. The stress suite checks 128 parses and 129 is refused.
(User.)

**Why:** a recursive-descent parser would overflow the stack and crash on a
hostile file.

**Rejected:** an explicit-stack parser with no limit; a configurable limit.

### D-S3-20 Byte-order marks and line endings

*(Line breaks inside multi-line strings: D-S3-43.)*

`Csv` and `Toml` skip one leading U+FEFF; encoders never write one; `Document`
keeps it. TOML parsing accepts CRLF; `Document` keeps each line's ending and a
line it inserts uses the ending of the table's last key line, else the file's
first; `Toml.encode` writes LF. CSV is unchanged (`Dialect.newline`). The README
says to prepend `"\u(FEFF)"` for Excel. (User.)

**Rejected:** a `bom` field on `Csv.Dialect`.

### D-S3-21 Build order

*(Temporal, CSV dates and trantor-hash move after stage 4: D-S3-37. Stage 1 has no `Bytes` types (D-S3-27); the `datetime` component lands in stage 3: D-S3-52.)*

1. Scaffold, `Base64`, `Hex`, the `Bytes` types.
2. CSV moved in.
3. TOML reading and the conformance suite.
4. Typed TOML and writing.
5. TOML editing.
6. Documentation.

(User.) trantor-temporal's part (D-S3-22, D-S3-25) follows stage 4.

**Rejected:** editing before typed TOML (editing needs only stage 3; typed has
more users).

### D-S3-22 Temporal's plain types decode and encode through format-neutral date methods

*(Extended to CSV and trantor-hash: D-S3-36.)*

Date-capable formats define `parse_local_date`/`encode_local_date`,
`parse_local_time`/`encode_local_time`,
`parse_local_datetime`/`encode_local_datetime` and
`parse_offset_datetime`/`encode_offset_datetime`. trantor-temporal gives
`PlainDate` and `PlainTime` hand-written `parser_for`/`encoder_for` requiring
the local date and local time methods, so `{ start : Temporal.PlainDate }`
decodes from TOML with no dependency between the packages. `Toml`'s D-S3-9
types use the same methods. There is no temporal type for local datetimes.
(User.)

**Why:** methods match by name, so a naming convention replaces an import, and
a later format with native dates (YAML, CBOR) supports temporal types by
defining the same names.

**Cost:** a type has one `parser_for`; temporal types in JSON as strings would
need a different design, such as a wrapper.

**Rejected:** conversions only, repeated in every config; trantor-temporal
importing trantor-encoding, which makes every temporal user compile it and ties
the methods to TOML.

### D-S3-23 The date methods carry temporal's record shapes

*(Signatures and the parse error row fixed by D-S3-26.)*

```roc
Toml.Date : { year : I32, month : U8, day : U8 }
Toml.Time : { hour : U8, minute : U8, second : U8, millisecond : U16, microsecond : U16, nanosecond : U16 }
Toml.Offset : { minutes : I16 }
parse_local_date : State -> Try({ value : Toml.Date, rest : State }, errs)
encode_local_date : Toml.Date, State -> Try(State, err)
# local_time with Time, local_datetime with { date, time }, offset_datetime with { date, time, offset }
```

A `Toml.Date` becomes a `PlainDate` with `Temporal.plain_date(d)`. Fraction
digits past nanoseconds are cut off on parse (a `Document` keeps the text). A
`PlainDate` on a non-ISO calendar encodes its ISO fields and drops the
calendar, documented. (User.)

Amends D-S3-8, whose first draft had `year : U16` and `nanosecond : U32` and
wrongly claimed those matched temporal.

**Rejected:** TOML-shaped records with temporal converting in its
`parser_for`.

### D-S3-24 TOML writing returns `EncodeErr`

*(Keys are segments: D-S3-30. Optional fields do encode: D-S3-37. Depth applies to typed encoding too: D-S3-44. Segment keys: D-S3-44.8. Time subfields over 999: D-S3-54.6.)*

```roc
Toml.EncodeErr : [
    InvalidDate({ key : List(Str), date : Toml.Date }),
    InvalidTime({ key : List(Str), time : Toml.Time }),
    InvalidOffset({ key : List(Str), minutes : I16 }),
    IntegerOutOfRange({ key : List(Str), value : Str }),
    RootNotATable,
    DuplicateKey(List(Str)),
    TooDeep(List(Str)),
]
```

- Runtime failures: a year outside 0–9999 or fields invalid on the ISO calendar
  (second 60 included), an offset outside ±23:59, an integer outside `I64`, a
  root that is not a table, and, for `Value` trees only, duplicate keys and
  nesting past 128. Writing stops at the first.
- Compile-time failures: no `encode_null`, no `encode_tag`, and only
  `encode_key_str`, so optional and tag-union fields and non-`Str` `Dict` keys
  do not compile.
- Never fails: any `Str`, `F32`/`F64` including NaN and infinity, `Dec` as its
  exact decimal text, `Bool`, tuples as arrays, empty records and lists.

(User asked whether a year was the only failure; the table above is the full
answer.)

**Rejected:** a crash on a bad year; clamping; separate error types for typed
and `Value` writing.

### D-S3-25 Offset datetimes to and from `ZonedDateTime` in trantor-temporal

*(The instant is kept when rounding the offset: D-S3-37.)*

```roc
Temporal.zoned_from_offset! : { date : Date, time : Time, offset : { minutes : I16 } } => Try(ZonedDateTime, Err)
to_offset_datetime! : ZonedDateTime => { date : Date, time : Time, offset : { minutes : I16 } }
```

`zoned_from_offset!` gives a fixed-offset zone on ISO — an instant, not a place.
`to_offset_datetime!` rounds `offset_seconds!` to minutes and drops the IANA
zone name; the README says to keep a zone name in its own key. Local datetimes
already convert with `Temporal.zoned!` and a zone from elsewhere. (User.)

**Why:** `ZonedDateTime` is made only by effects, so it cannot be a typed field;
formatting an offset string by hand invites a sign mistake under an hour.

**Rejected:** no functions; keeping the zone name in a comment or extra key.

### D-S3-26 The date methods' parse error is exactly `Mismatch`

*(`Mismatch.key` is a segment list: D-S3-30. Date encode methods take the format first, unlike their neighbours: D-S3-44.)*

```roc
parse_local_date : fmt, state -> Try({ value : { year : I32, month : U8, day : U8 }, rest : state }, [Mismatch({ key : List(Str), expected : Str }), ..])
encode_local_date : fmt, { year : I32, month : U8, day : U8 }, state -> Try(state, err)
# likewise local_time, local_datetime ({ date, time }), offset_datetime ({ date, time, offset })
```

The format is the first argument, as in the builtin methods. A date-capable
format's four `parse_*` date methods fail with `Mismatch` and no other tag; the
row may be open. A hand-written `parser_for` (temporal's, `Toml`'s D-S3-9 types)
names `[Mismatch({ key : List(Str), expected : Str })]` closed in its `where`
clause and reopens it with `? |Mismatch(m)| Mismatch(m)`. Encoding keeps an error
variable. (User.) Amends D-S3-22 and D-S3-23.

Found in review, measured on `10e922df`: a hand-written `parser_for` whose
`where` clause has an error variable (`err`, `[..errs]`) compiles at the top
level but fails inside a record or list, where the compiler asks the format
method for `[]`. Naming an exact row works nested (`M1n.roc`); one extra tag in
the format's row fails (`M1p.roc`). A generic `encoder_for` with an error
variable works nested (`E1c.roc`). Files in the S3 review scratch directory.

**Why `Mismatch`:** it is `Toml.decode`'s typed error already, carries the key
path, and only formats trantor writes define date methods.

**Rejected:** a looser `InvalidValue` without the path.

### D-S3-27 No `Bytes` types

`Base64.Bytes` and `Hex.Bytes` are dropped. Binary fields are `Str` and convert
with `Base64.decode`/`Hex.decode`. (User.) Withdraws D-S3-12.

**Why:** a `Bytes` type decodes through `parse_str`, and by D-S3-26's finding
one `parser_for` can name only one exact error row: `Json`'s builtin
`[InvalidJson(Str), ..]`, TOML's `Mismatch`, or CSV's. Tied to TOML it fails
JSON, the main place binary fields appear; tied to JSON it fails this package's
own formats; either way the compile error names `parse_str`'s type, not the
restriction. Encoding alone would work everywhere.

**Revisit:** if a hand-written `parser_for` with an error variable compiles
inside a record (repro `B64d.roc`).

**Rejected:** `Bytes` decoding from TOML only; from JSON only.

### D-S3-28 `MissingRequiredField` names the field, not the path

`Toml.decode`'s row carries `MissingRequiredField(Str)` as the compiler
generates it: the leaf field name. `Mismatch` alone carries a key path. The
README says a name repeated in two tables is ambiguous, and to decode that
table separately or give the field a default. (User.) Amends D-S3-11.

Found in review: the payload is fixed by the compiler
(`postcheck/monotype/lower.zig:47961`, "did not carry one Str payload"), and
the state holding the path is gone when the error returns. The compiler has an
`invalid_value : fmt, state -> err` hook that a parser may use when its public
row drops `MissingRequiredField` (`check/Check.zig:36645`); measured, it is
never selected on this build — not for the builtin `Json` with the result
annotated `[InvalidJson(Str)]`, nor for a format with a closed row, nor with
every error mapped by `?`.

**Rejected:** guessing the path from the parsed `Value` (wrong when several
tables lack the key); validating against field lists first (a format cannot
see them).

### D-S3-29 New tables choose a style through `Toml.Edit`

*(Styles, `Auto` and parent creation: D-S3-41.)*

```roc
Toml.Edit : { version ?: [V1_0, V1_1], table ?: [Auto, Inline, Header] }
set_with : Document, List(Toml.Segment), Toml.Value, Toml.Edit -> Try(Document, Toml.EditErr)
```

- `Auto` (default): inline when the parent is inline, the table is inside an
  array, or the parent `[header]` table already holds inline tables (`[deps]`);
  otherwise a `[header]` section.
- Parents that exist only to carry the path are implicit: setting
  `components.x.kind` in a document without `[components]` writes only
  `[components.x]`.
- `Inline`/`Header` force the style for the table set and everything nested
  in it; an inline table's descendants are inline.
- Replacing an existing table keeps its style unless `table` is given.
- `Toml.Write` keeps `version` for `to_str_with`/`encode_with`.

(User.)

Found in review against trantor's own editing: `trantor add` writes an inline
table (`src/registry.rs:368-377`) and `new-interface` writes header sections
with implicit parents (`src/new_interface.rs:63-77`); D-S3-10 could express
neither.

**Rejected:** separate `InlineTable`/`Table` variants on `Value`, which carry a
formatting detail through parsing and typed decode; always `Header` unless
told.

### D-S3-30 Paths are segment lists everywhere

```roc
Toml.Segment : [Key(Str), Index(U64)]
Toml.path : List(Str) -> List(Toml.Segment)       # keys only
```

`Document` operations, `Mismatch.key` (D-S3-11, and D-S3-26's cross-format
contract, spelled `List([Key(Str), Index(U64)])` there so other formats need
not import `Toml`) and every `EncodeErr` key use segments. `Index` addresses an
element of an array or array of tables; past the end is `NotFound`. (User.)

**Why:** `List(Str)` cannot reach `bin[1].name` in a `[[bin]]` array, and a
decode error inside a list could not say which element.

**Rejected:** `List(Str)` with `_at` functions for indices; string paths
(`"bin[1].name"`), which need escaping and runtime parsing.

### D-S3-31 `remove` takes the whole span; one `EditErr`

*(Removal by path prefix: D-S3-40. Empty paths: D-S3-54.8.)*

`remove` takes:

- a key-value (top level, header, dotted): the key through the end of its
  value, multi-line strings and arrays included, the rest of that line with its
  comment, and the comment lines directly above (a blank line stops them);
- an entry in an inline table or array: the entry and one adjacent comma, or
  its lines when the container is multi-line;
- a `[header]` table: its header, key-values, comment block and every
  `[a.b…]` section below it, keeping unrelated sections in between;
- an `Index` into `[[x]]`: that section.

Removing a table's last key leaves the table.

```roc
Toml.EditErr : [
    NotFound(List(Toml.Segment)),
    NotATable(List(Toml.Segment)),
    NotAnArray(List(Toml.Segment)),
    Encode(Toml.EncodeErr),
]
get : Document, List(Toml.Segment) -> Try(Toml.Value, Toml.EditErr)
set, set_with, remove, append -> Try(Document, Toml.EditErr)
```

`append` to `[[x]]` adds a section after the last `[[x]]` block; to an array
value it follows the array's layout (one line stays one line; a multi-line
array gains a line indented like the last element, keeping a trailing comma).
Checking for a duplicate is the caller's, with `get`. (User.) Amends D-S3-10.

**Rejected:** a row per function, which a chain of `?` must widen.

### D-S3-32 `Toml.Value` is nominal, with document equality

*(Floats keep their spelling: D-S3-38. Sorted comparison: D-S3-44.)*

`Toml.Value := [...]` (a recursive alias does not compile; bare tags still
construct it). Its `is_eq`: tables equal when they hold the same keys with equal
values, in any order; arrays element-wise in order; floats equal when both are
NaN or `==` holds. Iteration and output keep document order. (User.)

- `parse(to_str(v)) == v` under this equality, in both write modes; `Document`
  round trips stay byte-identical.
- `parse_dec` reads the literal's source text, so a typed `Dec` round-trips
  exactly; a `Value` holds `F64`, and the README says digits past its precision
  are lost there (a `Document` keeps the text).
- Writing sorts `Dict` keys by their bytes, for D-S3-17.

Found in review: D-S3-15's order reorders `a.b = 1` / `c = 2`; `nan != nan`;
two equal `Dict`s iterate in different orders (measured).

**Rejected:** structural equality with a writer that preserves order through
dotted keys; a `Decimal(Dec)` variant or source text in `Value`.

### D-S3-33 `Value` as a field; `decode_value` and `encode_value`

*(`decode` repeats `Parse` in its where clause: D-S3-38.)*

`Toml.Value` has hand-written `parser_for`/`encoder_for` tied to TOML's format
(tied to one format works inside records; any other format is a compile error
naming TOML's format).

```roc
Toml.decode_value : Toml.Value -> Try(a, [Mismatch({ key : List([Key(Str), Index(U64)]), expected : Str }), MissingRequiredField(Str), ..]) where [...]
Toml.encode_value : a -> Try(Toml.Value, Toml.EncodeErr) where [...]
```

`decode` is `parse` then `decode_value`. A `Document` decodes through
`to_value`; a typed record goes into a document through `encode_value` and
`set`. (User.)

**Rejected:** neither, which sends `trantor add`-style edits through printing
and parsing.

### D-S3-34 One depth: 128 over every kind of nesting

*(Edit depth errors arrive as `Encode(TooDeep)`: D-S3-54.9.)*

A value's depth counts every enclosing table and array however written —
headers, dotted keys, inline tables, arrays, arrays of tables (`[a.b]` then
`c.d = [1]` puts `1` at 5). `parse`, `parse_document` and `decode` refuse past
128 with `OutOfRange` at the key or bracket that crosses it, `text` saying
`nesting deeper than 128`; `set`, `append` and `encode_value` answer `TooDeep`;
`to_str` checks too, which a parsed value never trips. (User.) Supersedes
D-S3-19.

Found in review: headers and dotted keys built unbounded depth that equality,
writing and decoding then recurse over (measured: a recursive walk crashes at a
million levels); a parsed value past 128 made `to_str` answer `TooDeep`.

**Rejected:** a higher separate cap for headers and dotted keys.

### D-S3-35 Conformance runs the 1.1 lists, compared by value

*(1.0.0 lists and the checker's own test: D-S3-44. CRLF corpus files kept by `.gitattributes`: D-S3-54.1.)*

The 1.1.0 invalid and valid lists run; the 1.0.0 lists do not (1.1.0's valid
list contains 1.0.0's). A test-only strict 1.0 checker parses every default-mode
output and rejects 1.1-only syntax, guarding D-S3-7's promise. Expected typed
JSON converts to `Toml.Value` and compares under D-S3-32's equality. The suite
README records the corpus commit and the case counts per list. (User.)

Found in review at toml-test `ff49d109`: nine 1.0.0 invalid files are valid 1.1
(`inline-table/linebreak-01..04`, `inline-table/trailing-comma`,
`{datetime,local-datetime,local-time}/no-secs`, `string/basic-byte-escapes`),
and expected JSON normalizes spellings (`1e06` → `"1e+06"`).

**Rejected:** a 1.0 read mode so both invalid lists run.

### D-S3-36 CSV and trantor-hash support the date methods

*(CSV dates fail with `Mismatch` like TOML's: D-S3-39.)*

- CSV defines all eight. A cell reads as RFC 3339 (`2026-03-08`, `07:32:00`,
  `2026-03-08T07:32:00`, `…-08:00`; a space for `T` and missing seconds
  accepted) and writes as TOML's default mode does. Its four parse methods fail
  with `Mismatch` (D-S3-26); `Csv.decode` folds that into `BadCells`.
- trantor-hash's `HashFormat` defines the four encode methods, hashing fields
  as fixed-width integers in order, pinned by D-S1-6's golden expects; no
  existing hash changes.
- JSON stays a compile error, documented in trantor-temporal's README.

(User.) Extends D-S3-22.

Found in review: D-S3-22 recorded JSON as its cost, but CSV (date columns) and
`Hash.of` a record with a date were excluded too.

**Rejected:** neither; CSV only.

### D-S3-37 Review corrections

*(Item 10 refined by D-S3-42; item 4 by D-S3-43.)*

(User, accepting all.)

1. trantor-temporal adds `plain_date_from_fields : { year : I32, month : U8,
   day : U8 } -> PlainDate` and `plain_time_from_fields` (a `Toml.Date`-typed
   value does not pass to `Temporal.plain_date`); amends D-S3-23.
2. Optional (`?:`) fields encode by omission; only tag-union fields, `Try`
   included, fail to compile; amends D-S3-24.
3. `V1_1` changes only `\e`/`\xHH` escapes and seconds omitted when zero.
   Inline tables and arrays stay on one line in both modes (newlines in inline
   tables are the 1.1 feature; trailing commas and multi-line arrays are 1.0);
   amends D-S3-16.
4. Escaped in strings: `"`, `\`, U+0000–001F, U+007F; `\b \t \n \f \r` where
   named, else `\u00XX` (1.0) or `\e`/`\xHH` (1.1); all else literal UTF-8.
   Multi-line strings write newlines literally.
5. Second 60 is refused on parse (`OutOfRange`) as on write, documented as a
   deliberate departure from the ABNF.
6. Floats: the shortest spelling that reads back identically and is
   recognisably a float — `1.0`, `-0.0`, `1e300`, `1e-7`, `inf`, `-inf`,
   `nan` — with golden cases (`F64.to_str(1.0)` is `"1"`).
7. Base64: length ≡ 1 mod 4 is `InvalidLength`; indices (Base64 and Hex) count
   UTF-8 bytes; "one valid encoding" reads "the payload bits are canonical;
   padding is optional"; amends D-S3-2.
8. An integer decodes into `F32`/`F64`/`Dec` when exact; a float into an
   integer field is `Mismatch`; amends D-S3-9.
9. `to_offset_datetime!` converts the instant to the rounded offset and
   recomputes the wall clock; amends D-S3-25.
10. `set` always writes the 1.0 subset; `set_with` chooses; amends D-S3-16.
11. trantor-temporal, CSV's date methods and trantor-hash's land after stage 4,
    before editing; amends D-S3-21.
12. Tests added: date and `Value` codecs inside records, lists and dicts
    against two formats; `I64` bounds (`-9223372036854775808` accepted,
    `9223372036854775808` and `0x8000000000000000` refused); offset overflow;
    multi-line strings with `"""`, a trailing `"`, a leading newline, `\r`;
    setting a scalar over a `[section]` and the reverse; removing a table's
    last key; columns after astral characters and CRLF; `[[x]]` after a
    static array of the same name.

### D-S3-38 `Value` floats keep their spelling: `Toml.Float`

*(Digits past `Dec`'s 18: D-S3-48. Spelling wording: D-S3-54.10.)*

```roc
Toml.Value := [ …, Float(Toml.Float), … ]
Toml.Float :: …                                   # the literal as written, or as generated
to_f64 : Toml.Float -> F64
to_dec : Toml.Float -> Try(Dec, [NotADec])        # inf, nan, out of Dec's range
Toml.float_from_f64 : F64 -> Toml.Float           # shortest round-trip spelling (D-S3-37.6)
Toml.float_from_dec : Dec -> Toml.Float           # exact decimal text
```

`parse_dec` reads the kept spelling, so `Dec` is exact through `Value`,
`decode_value` and `encode_value`; `decode` stays `parse` then `decode_value`
(D-S3-33). Two floats are equal when their `F64`s are (NaN equal to NaN), so
`1.0 == 1.00`. `to_str` writes the kept spelling when it is valid for the write
mode, else the shortest `F64` spelling. `Toml.decode`'s `where` clause repeats
`Parse(Toml.Err)` (measured: without it, a type mismatch). (User.) Amends D-S3-8
and D-S3-32.

Found in review round 2: `decode` through a `Value` holding `F64` could not give
`parse_dec` source text; there is no `F64 → Dec` conversion, and
`12345678.123456789012345678` came back `12345678.12345679` (measured).

**Why not a tree:** decoding over `Document`'s internal tree kept `Dec` exact
only outside `Value`, and a typed record stored through `encode_value` would lose
digits. The loss came from `Float(F64)`, a choice, not a constraint.

**Rejected:** exact `Dec` dropped; a private spelling tree beside `Value`; a
`Decimal(Dec)` variant (D-S3-32).

### D-S3-39 CSV takes TOML's error model

*(Header union D-S3-45, duplicates D-S3-49, cell grammar D-S3-50, dates D-S3-51, row index D-S3-54.3, quote errors D-S3-54.4, error cases declared early D-S3-54.2. `encode_columns_with` has `encode_columns`'s error row.)*

(User: CSV "was written without a lot of thought about the API and has no real
consumers. It should be adjusted as needed to match the other parsers.")

```roc
Csv.Err : [
    Syntax({ line : U64, column : U64, expected : Str }),
    RaggedRow({ line : U64, expected : U64, found : U64 }),
    MissingHeader,
]
Csv.err_to_str : Csv.Err -> Str
Csv.decode : Str -> Try(List(a), [Parse(Csv.Err), Mismatch({ key : List([Key(Str), Index(U64)]), expected : Str }), MissingRequiredField(Str), ..])
    where [a.parser_for : Csv.Format -> (Csv.State -> Try({ value : a, rest : Csv.State }, [Parse(Csv.Err), Mismatch(…), MissingRequiredField(Str), ..errs]))]
Csv.EncodeErr : [InvalidDate({ key : List([Key(Str), Index(U64)]), date : … }), InvalidTime(…), InvalidOffset(…)]
Csv.encode : List(a) -> Try(Str, Csv.EncodeErr) where [...]
Csv.encode_with : List(a), Csv.Dialect -> Try(Str, Csv.EncodeErr) where [...]
Csv.encode_columns : List(a), List(Str) -> Try(Str, [UnknownColumn(Str), MissingColumn(Str), Encode(Csv.EncodeErr)]) where [...]
```

Decoding stops at the first error; `BadCells` and `Csv.Bad` are gone. A cell's
key is `[Index(row), Key(column)]`, rows counted from 0 after the header. CSV's
date methods fail with `Mismatch` like TOML's. Dates write as RFC 3339 with a
four-digit year; a year outside 0–9999, invalid fields or an offset past ±23:59
are `EncodeErr`. Nothing else fails to write. Lines are 1-based, columns in code
points. `_with` takes `Csv.Dialect`. Amends D-S3-4, D-S3-5, D-S3-36.

Found in review round 2: folding `Mismatch` into `BadCells` did not compile with
an open row (measured), CSV's record-and-continue scalars made a failing date
method lose the row's other errors, and `Csv.encode : List(a) -> Str` could not
report a year TOML-style output cannot write.

**Rejected:** collecting every bad cell as CSV's one difference; closed rows
with `Mismatch` handled internally.

### D-S3-40 `remove` deletes by path prefix

*(Kind changes: D-S3-47.)*

Removing a path deletes every key-value and section whose full path starts with
it, wherever it is: `[a]`, `[a.b]` before or after it, `a.x = …` lines in any
table, `[[a.list]]` sections — each with its own comment block per D-S3-31.
Unrelated sections between them stay. "Removing a table's last key leaves the
table" holds only for header and inline tables; a table made only of dotted keys
or implied by deeper headers disappears from `to_value` with its last key, and
the README says so. (User.) Amends D-S3-31.

Found in review round 2: dotted tables spread over a file (the spec's
`apple`/`orange` example), implicit tables and sub-sections before their parent
were not covered.

**Rejected:** refusing to remove a table whose pieces are not contiguous.

### D-S3-41 Table styles: `Auto`, `Inline`, `Header`, `Dotted`

*(Header-created and dotted-created tables: D-S3-46. Implicit parents and `[[x]]` elements: D-S3-54.7.)*

```roc
Toml.Edit : { version ?: [V1_0, V1_1], table ?: [Auto, Inline, Header, Dotted] }
Toml.EditErr gains StyleNotPossible({ key : List(Toml.Segment), style : [Header, Dotted] })
```

`Auto`, per new table from the top down:

- an inline parent or enclosing array value → `Inline`;
- a dotted parent → `Dotted`;
- a `[header]` parent, not the root, whose existing children are all inline
  tables and which has no `[p.x]` sub-sections → `Inline`;
- otherwise `Header` (the root, an empty header, a parent with sub-sections);
- an array of tables under the root or a header → `[[x]]` sections; under an
  inline parent → an inline array.

`Inline`/`Header`/`Dotted` apply to the table set; tables nested in an inline
table are inline, and in a header or dotted table follow `Auto`. `Header` or
`Dotted` under an inline parent or inside an array is `StyleNotPossible`. A
missing direct parent of an `Inline` or `Dotted` table is created as a
`[header]`; parents above it stay implicit. `trantor add`'s first dependency
under `Auto` is `[deps.foo]`; a tool wanting `foo = { … }` passes `Inline`.
(User.) Amends D-S3-29.

Found in review round 2: impossible `Header` requests, the lost dotted-parent
rule, mixed Cargo-style parents, and `bin = [{…}]` against `encode`'s `[[bin]]`.

**Rejected:** falling back silently to the nearest possible style.

### D-S3-42 Edits follow existing layout; `version` governs new constructs

An edit inside a construct already in the document follows its layout, 1.1
included: a new entry in a multi-line inline table takes its own line (and a
trailing comma when its neighbours have one); a new element in a multi-line
array likewise. What an edit creates — escapes, times, a new inline table —
follows `version`, `V1_0` by default. An edit never makes a document need 1.1
unless `V1_1` is given. (User.) Refines D-S3-37.10.

Found in review round 2: "`set` always writes 1.0" contradicted following a
neighbouring 1.1 construct.

**Rejected:** rewriting a touched 1.1 construct as 1.0, which reformats the
user's lines.

### D-S3-43 Line breaks inside multi-line strings read as `\n`

In multi-line basic and literal strings a line break reads as `\n` whether the
file has LF or CRLF; an escaped `\r` stays a carriage return. `\r` is always
escaped on write; a multi-line string's breaks are written as the file's line
ending (LF from `encode`/`to_str`, the surrounding ending in a `Document`
edit). `Document` keeps raw bytes; only `to_value`/`get` see the normalized
string. (User.) Amends D-S3-20 and D-S3-37.4.

Found in review round 2: TOML 1.1 lets parsers normalize these, the corpus pins
nothing, and keeping CRLF makes values depend on the checkout.

**Rejected:** keeping CRLF as written.

### D-S3-44 Review round 2 corrections

*(Item 10 replaced by D-S3-51.)*

(User, accepting all.)

1. `Value`'s `is_eq` sorts entries by key before comparing; the parser and
   `Document` find duplicates with a `Dict`; the 10,000-key stress test has a
   time budget (a reversed compare took 5.6 s in `roc test`, measured in
   review).
2. Conformance also runs the 1.0.0 valid list through `parse` (its 48
   `spec-1.0.0` files are not in 1.1.0's list); the strict 1.0 checker is
   tested against both 1.0.0 lists; the suite has a hand-written JSON reader
   (test code). Amends D-S3-35.
3. Date encode methods take the format first; the other `encode_*` methods do
   not (measured: derived containers and leaf encoders with a format argument
   are arity errors). Formats with dates mix both forms. Amends D-S3-26.
4. The four TOML date types and `Toml.Float` define `is_eq` (a nominal without
   it does not support `==`, measured).
5. A `Toml.Edit` kept in a variable is annotated `e : Toml.Edit` (measured:
   unannotated fails); the README says so.
6. Sorted comparison keeps equality symmetric with user-built duplicate keys
   (`to_str` still answers `DuplicateKey`); golden tests check `-0.0`, which
   round trips cannot.
7. Fractions write with trailing zeros removed, at most 9 digits, omitted when
   zero; `V1_1` omits seconds only when seconds and fraction are both zero.
8. `Toml.Err.Duplicate`, `EncodeErr.DuplicateKey` and `TooDeep` carry
   `List(Toml.Segment)`, with `Index` inside arrays of tables.
9. Bare keys are non-empty; the empty key writes as `""`.
10. CSV date cells: RFC 3339, the space for `T` (RFC 3339 §5.6), and missing
    seconds as TOML 1.1's extension; CSV date tests use records only.
11. Base64 scans left to right: the first bad character or misplaced `=`
    (partial padding such as `QQ=` at the `=`) is `InvalidBase64`; only a clean
    scan checks `InvalidLength`. Hex: `InvalidHex` before `OddLength`.
12. `Toml.encode` and `encode_value` answer `TooDeep` for deep recursive user
    types too; amends D-S3-24.
13. Tests: CSV's first bad cell with its `[Index, Key]` path; `Csv.encode` of a
    year-10000 date; LF and CRLF checkouts with equal values; removing dotted,
    implicit and before-the-parent sections; `Auto` with an array of tables and
    `trantor add`'s first dependency under `Auto` and `Inline`; equality with
    duplicate keys, nested NaN and the timed 10,000-key compare; golden
    fraction digits and an empty key; the strict checker on both 1.0.0 lists.

### D-S3-45 CSV's header is the union of every record's fields

`Csv.encode`'s header is the union of field names across all records, sorted by
bytes; an absent optional field is an empty cell, placed by name. An empty cell
decodes into a `?:` field as absent. `encode_columns` answers `UnknownColumn`
for a name no record has — a misspelling and an optional field absent from every
record cannot be told apart from values, and the README says to leave such a
column out — and `MissingColumn` for a field some record has that is not named.
(User; the `UnknownColumn` choice was stated after the answer and not
disputed.)

Found in review round 3, measured: `{ a : U64, b ?: Str, c : Str }` rows encode
with different field lists (`a|b|c`, `a|c`), and the playground's first-record
header put `z` under `b` silently.

**Rejected:** requiring equal field lists (`FieldsDiffer`); empty columns for
unseen names.

### D-S3-46 Tables take additions only in the form that created them

A table named in any header path (`[p.x.y]` names `p`, `p.x`, `p.x.y`) is
header-created: new keys go in its own `[p.x]` section, and `Dotted` for a key
or table under it (other than the header's own table) is `StyleNotPossible`;
`Auto` never chooses it. A table defined by dotted keys takes new sub-tables as
`Dotted` or `Inline`; `Header` there is `StyleNotPossible`. Replacing a
header-created table with `Inline` first removes its pieces by prefix
(D-S3-40). Every `tests/toml-edit` case also checks that the output parses and
`get(path)` equals the value set (`NotFound` after a removal). (User.) Amends
D-S3-41.

Found in review round 3: `[p.x.y]` then `x.k = …` under `[p]` is invalid
(toml-test `invalid/table/append-with-dotted-keys-01/02/08`), and D-S3-41's
dotted rules could write it; snapshots compared bytes only.

**Rejected:** re-parsing every edit and answering `WouldBeInvalid`.

### D-S3-47 `set` across kinds removes and re-adds, keeping position where it can

- Table → scalar: every piece under the path is removed by prefix (D-S3-40),
  comments with them; the scalar is added to the parent after its last key.
- Scalar → table: `Auto` replaces the line in place as `Inline`; `Dotted`
  replaces it in place; `Header` removes the line and adds a section by
  D-S3-53. A one-line result keeps the line's trailing comment.
- An `Array` counts as a scalar against `[[x]]`.

(User.) Amends D-S3-10.

Found in review round 3: the plan tested both directions with no rule for
either.

**Rejected:** `KindChanged`, making callers `remove` then `set`.

### D-S3-48 `Dec` cuts digits past 18, toward zero

`to_dec` and `parse_dec` apply an exponent, then keep 18 fractional digits and
drop the rest without rounding (`0.9999999999999999999` →
`0.999999999999999999`, `1e-30` → `0.0`). `NotADec` is only a whole part
beyond `Dec`'s range, `inf` and `nan`; `decode` answers `Mismatch` with
`expected` naming `Dec`. `Toml.Float`'s spelling is untouched. (User.) Amends
D-S3-38.

Found in review round 3, measured: `Dec.from_str` refuses 19 and 20 fractional
digits and `0.000…001`, which an `F64` field accepts.

**Rejected:** `Mismatch` for over-precise literals.

### D-S3-49 A repeated CSV header name is an error

`Csv.Err` gains `DuplicateHeader({ line : U64, column : U64, name : Str })` at
the second occurrence, byte-exact, in `table`, `decode` and their `_with`
forms; `parse` has no header. One empty header name is allowed; a second is a
duplicate. `encode_columns` given a name twice answers `DuplicateColumn(Str)`.
(User.)

Found in review round 3, measured: `decode("a,a\n1,2")` gave the last value and
`Row.get` the first.

**Rejected:** first wins; keeping duplicates in `Table` and refusing only on
decode.

### D-S3-50 CSV numbers and booleans use XML Schema's lexical forms

The W3C CSV on the Web Recommendation parses typed cells in XML Schema 1.1's
formats by default; Frictionless Table Schema is close. RFC 4180 defines no
types.

- Integers `[+-]?[0-9]+`, range-checked.
- `F32`/`F64`/`Dec`: sign, digits with an optional fraction (`.5`, `5.`),
  optional `e`/`E` exponent; `F32`/`F64` also `INF`, `+INF`, `-INF`, `NaN`,
  case-sensitive; `Dec` refuses those.
- `Bool`: `true`, `false`, `1`, `0`.
- Writing uses `INF`, `-INF`, `NaN`, `true`, `false`.
- Underscores, hex, grouping and other spellings are `Mismatch`; whitespace
  only through `Dialect.trim`. TOML keeps its own grammar.

(User, after asking whether a CSV standard covers this.)

Found in review round 3, measured: Roc's `from_str` accepts `1_000`, `+5`,
`0x10` and `nan`, and could change under D-S3-17.

**Rejected:** a strict grammar of our own with lowercase specials; Frictionless's
case-insensitive specials and three boolean casings; Roc's `from_str`.

### D-S3-51 CSV dates use XML Schema 1.1's forms

- `date` `-?YYYY-MM-DD`, `time` `hh:mm:ss(.fraction)?`, `dateTime`
  date`T`time, each with an optional timezone `Z` or `±hh:mm` within ±14:00;
  `T` and `Z` uppercase, seconds required.
- `local_date`/`local_time`/`local_datetime` read the forms without a
  timezone; `offset_datetime` reads `dateTime` with one. A timezone on a local
  field, none on an offset field, lowercase `t`/`z`, a space for `T` and missing
  seconds are `Mismatch`.
- Fractions of any length cut after nanoseconds; second 60 and `24:00:00`
  refused; years of four or more digits, negative and `0000` allowed, outside
  `I32` a `Mismatch`; `-00:00` is offset 0.
- Writing: years outside 0–9999 in the extended form (`10000-01-01`,
  `-0044-03-15`); an offset beyond ±14:00 is `InvalidOffset`; fractions trimmed
  (D-S3-44.7); `Z` for offset 0. `InvalidDate` is left for invalid fields only.

(User.) Replaces D-S3-44.10; amends D-S3-36 and D-S3-39.

**Rejected:** TOML's date rules for CSV (no CSV standard uses them, and
years outside 0–9999 would not write).

### D-S3-52 Shared date logic in an internal `datetime` component

```toml
[components.datetime]
kind = "roc"
exports = ["EncodingDate"]      # not a package export
```

`EncodingDate` holds the `Date`/`Time`/`Offset` records, calendar and range
checks, fraction reading and writing, and a fixed-width digit cursor. CSV's XML
Schema grammar and TOML's RFC 3339 grammar stay in their components.
`Toml.Date`/`Time`/`Offset` alias its records. `csv` and `toml` import
`datetime`; replacing one format does not touch the other. Built in stage 3.
(User.) Amends D-S3-13.

Found in review round 3: where the shared code lived was unspecified, and CSV
importing a `toml` module would break D-S3-13.

**Rejected:** duplicating the calendar rules per format.

### D-S3-53 New sections go after their family's last section

A new `[section]` or `[[x]]` goes after the last section whose path starts with
the new section's parent path (with its sub-sections), the deepest such parent
deciding; with no family, at the end of the file. One blank line before it,
unless the family has none between its sections (D-S3-42). The same rule places
sections created by D-S3-46 and D-S3-47. (User.) Amends D-S3-10.

Found in review round 3: "at the end" split trantor's own `[components.*]`
sections from each other.

**Rejected:** always the end of the file.

### D-S3-54 Review round 3 corrections

(User, accepting all.)

1. `.gitattributes` with `-text` for `tests/toml-conformance/corpus/**` and
   `tests/toml-edit/cases/**` in stage 3, and an expect that a CRLF corpus file
   holds `\r\n` (the user's `core.autocrlf=input` would convert them, measured).
2. `Csv.EncodeErr`'s date cases are declared in stage 2 (an empty error row
   makes an `Err` branch an "unmatchable pattern" warning, measured).
3. CSV `Index(row)` counts data records from 0 after the header; skipped blank
   and comment lines do not count; a multi-line quoted cell is one record.
   `Mismatch` stays `{ key, expected }`; the README points to `Csv.table` to find
   the record.
4. CSV quote errors: an unterminated quote at its opening quote; text after a
   closing quote and a quote in an unquoted field at the offending character;
   `expected` names what would fix it.
5. Base64: data after complete padding at its first character, excess padding
   at the first `=` that cannot be there, nonzero trailing bits at the last data
   character, all `InvalidBase64`; the row is `[InvalidBase64(U64),
   InvalidLength]`.
6. `InvalidTime` includes a millisecond, microsecond or nanosecond over 999.
7. `Auto`: a table without keys of its own is the "otherwise `Header`" case; a
   table inside a `[[bin]]` element is under a header (`[bin.sub]` after that
   element); an array of tables under a dotted parent is inline.
8. Empty paths: `get` returns the root; `set` takes only a `Table` (else
   `Encode(RootNotATable)`) and replaces the document; `remove`/`append` answer
   `NotFound([])`; `Toml.path([])` is `[]`.
9. Depth errors from edits arrive as `EditErr.Encode(TooDeep(path))`.
10. D-S3-38 writes the kept spelling (float syntax is the same in 1.0 and 1.1);
    `F32` writes its own shortest spelling; `-0.0` gets the `.0` fix.
11. `Mismatch.expected` and `err_to_str` texts are message text outside
    D-S3-17; tests check tags and paths, and any golden message is marked
    unpromised.
12. `Try` fields decode from TOML and CSV but do not encode (D-S3-37.2);
    documented, and left out of round-trip tests.
13. The README shows building a `Value`, including
    `Float(Toml.float_from_f64(1.5))`.
14. Stale wording is marked on D-S3-11, D-S3-14, D-S3-15, D-S3-21, D-S3-24,
    D-S3-39.
15. Tests: mixed-presence CSV rows; each D-S3-46 shape; kind changes both ways;
    19- and 20-digit `Dec`; duplicate headers; each XML Schema number, boolean
    and date form and the refused ones; D-S3-53 placements; the Base64 cases
    above; an empty path per edit function; the CRLF corpus check.

## Still open

- **`invalid_value`:** if the compiler starts selecting it, D-S3-28 can carry a
  path.
- **`Bytes` types:** revisit if a hand-written `parser_for` with an error
  variable compiles inside a record (D-S3-27).
- **The cross-package miscompile** the playground recorded on `e2b81982` is
  unmeasured on `10e922df`; stage 2 runs the CSV assertions through a package.
