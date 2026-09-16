# S3 — `trantor-encoding`

**Design log:** `notes/2026-09-16-s3-encoding-design-log.md`, D-S3-1 to D-S3-56.
Where a decision is amended, the later one governs; each amended decision names
its amendments at the top. The effective error and typed signatures are the
block in D-S3-55; the shared component is D-S3-56. Repos:
`~/dev/roc/trantor-encoding` (new), `~/dev/roc/trantor-temporal`,
`~/dev/roc/trantor-hash`, `trantor` (docs). Source for CSV:
`~/dev/roc/playground/csv`, copied, never modified or deleted (D-S3-18); its API
is not preserved (D-S3-39). Gate at every commit: `trantor test .` in each
touched package passes with no warnings.

**Working rule:** implementation proceeds without stopping to ask. When a
measurement contradicts the log, a case is undefined, or a planned construct
does not compile, record the finding and the choice made in Implementation
notes, pick the option most consistent with the log, and continue, committing
at each step.

## Why

D-S1-11 step 3. Nothing in Roc reads or writes TOML or Base64; hex exists only
for builtin digests; the CSV package sits outside trantor. trantor's own
manifests are TOML, and a Roc tool that edits them needs a lossless document.

## Package

```
trantor-encoding/
  package.toml
  README.md
  .gitattributes          # -text for the corpus and edit cases (D-S3-54.1)
  components/
    base64/  Base64.roc Hex.roc                          # no imports
    common/  EncodingDate.roc EncodingNumber.roc EncodingText.roc EncodingPath.roc
    csv/     Csv.roc CsvParse.roc CsvEmit.roc CsvCell.roc CsvDate.roc ... (from the playground)
    toml/    Toml.roc TomlLex.roc TomlParse.roc TomlFormat.roc TomlWrite.roc TomlDocument.roc TomlDate.roc ...
  tests/
    toml-conformance/  app.roc test.sh corpus/ README.md (commit, case counts)
    toml-edit/         app.roc test.sh cases/<name>/{before.toml,edit,after.toml}
    readme/            main.roc expected
```

```toml
[package]
name = "trantor-encoding"
version = "0.1.0"
exports = ["Base64", "Hex", "Csv", "Toml"]

[components.base64]
kind = "roc"
exports = ["Base64", "Hex"]

[components.common]
kind = "roc"
exports = ["EncodingDate", "EncodingNumber", "EncodingText", "EncodingPath"]   # not package exports

[components.csv]
kind = "roc"
exports = ["Csv", "CsvParse", "CsvEmit"]      # plus whatever internal modules result

[components.toml]
kind = "roc"
exports = ["Toml", "TomlLex", "TomlParse"]    # plus whatever internal modules result

[dev-deps]
trantor-cli = { path = "../trantor-cli" }
```

Internal modules are prefixed with their format (or `Encoding` in `common`) and
are component exports only. `csv` and `toml` import `common`; neither imports
the other; `base64` imports nothing. File layout inside a component is the
implementer's; the public surface is not.

## Surface

Signatures by decision:

| Module | Decisions |
|---|---|
| Base64 | D-S3-2, D-S3-17, D-S3-37.7, D-S3-44.11, D-S3-54.5, D-S3-55.12, D-S3-56 |
| Hex | D-S3-3, D-S3-17, D-S3-37.7, D-S3-44.11, D-S3-55.13 |
| common (internal) | D-S3-52, D-S3-56 |
| Csv | D-S3-4, D-S3-5, D-S3-17, D-S3-20, D-S3-36, D-S3-39, D-S3-45, D-S3-49, D-S3-50, D-S3-51, D-S3-54.2–4, D-S3-55.3–6, .8–9, .11, .14–15, .18 |
| Toml values and errors | D-S3-8, D-S3-11, D-S3-24, D-S3-28, D-S3-30, D-S3-32, D-S3-38, D-S3-44.8, D-S3-48, D-S3-54.6, D-S3-55.1–2, .4–5, .10 |
| Toml typed | D-S3-9, D-S3-33, D-S3-38, D-S3-48, D-S3-54.10, .12, D-S3-55.7, .18 |
| Toml writing | D-S3-15, D-S3-16, D-S3-17, D-S3-37.3–6, D-S3-43, D-S3-44.7, .9, .12, D-S3-54.10, D-S3-55.16 |
| Toml editing | D-S3-10, D-S3-29, D-S3-31, D-S3-40, D-S3-41, D-S3-42, D-S3-46, D-S3-47, D-S3-53, D-S3-54.7–9, D-S3-55.17 |
| Dates | D-S3-23, D-S3-26, D-S3-44.3, D-S3-55.4, D-S3-56 |

Format types are nested in their module (measured: `Fmt :: [].{ Enc := … }`
with the entry point on `Fmt`). Entry points construct the format's initial
state, so they live in the module that defines it; the generic parts go through
`EncodingPath.run`/`encode_run` (D-S3-56, measured across components).

Error payload fields holding segment lists are named `path` (D-S3-55.4).
`Toml` and `Csv` each define `a.Parseable(errs)` and `a.Encodable(err)` and use
them in typed signatures (D-S3-55.18).

### The date contract (D-S3-22, D-S3-23, D-S3-26, D-S3-30, D-S3-36, D-S3-44.3, D-S3-55.4)

Defined by TOML's and CSV's formats; the `encode_*` half also by trantor-hash's
`HashFormat`:

```roc
parse_local_date : fmt, state -> Try({ value : { year : I32, month : U8, day : U8 }, rest : state }, [Mismatch({ path : List([Key(Str), Index(U64)]), expected : Str }), ..])
encode_local_date : fmt, { year : I32, month : U8, day : U8 }, state -> Try(state, err)   # format first, as encode_key_* are; value and container encoders are not
parse_local_time / encode_local_time             # { hour : U8, minute : U8, second : U8, millisecond : U16, microsecond : U16, nanosecond : U16 }
parse_local_datetime / encode_local_datetime     # { date, time }
parse_offset_datetime / encode_offset_datetime   # { date, time, offset : { minutes : I16 } }
```

- The parse row holds `Mismatch` and no other tag. A hand-written
  `parser_for` names the row closed in its `where` clause and reopens it
  (`? |Mismatch(m)| Mismatch(m)`); an error variable there compiles at the top
  level and fails inside records.
- The date encode methods take the format first, as `encode_key_*` do; value
  and container encoders do not.
- Encoding keeps an error variable.
- The text each format reads and writes differs: TOML uses RFC 3339 with TOML's
  rules (D-S3-37, D-S3-44.7); CSV uses XML Schema 1.1 (D-S3-51). Calendar,
  range (limits as parameters) and fraction logic is shared in `EncodingDate`
  (D-S3-52, D-S3-56).
- `Toml`'s four date types work with any date-capable format and are tested
  through CSV records too (D-S3-55.7).

### Measure-inside-containers rule

Every hand-written `parser_for`/`encoder_for` (TOML's four date types,
`Toml.Value`, temporal's `PlainDate`/`PlainTime`) is tested inside a record, a
list, a nested record and a `Dict` value, never only at the top level. Dates are
also tested through CSV, in records (CSV has no lists or dicts).

## Work — commit at each

Order: 1 Base64/Hex, 2 `common`, 3 CSV, 4 TOML reading, 5 typed TOML and
writing, 6 dates across packages, 7 TOML editing, 8 documentation. `common` is
built whole in step 2, before CSV, because CSV uses `EncodingText`,
`EncodingNumber`, `EncodingPath` from its first commit and declares date
payloads in step 3; nothing depends on `common` yet in step 2, so its own
expects keep that commit green, and every later step only adds a dependent.
This supersedes D-S3-52's "built in stage 3" (D-S3-56).

1. **Scaffold, Base64, Hex** (D-S3-2, D-S3-3, D-S3-17, D-S3-27, D-S3-37.7,
   D-S3-44.11, D-S3-54.5, D-S3-55.12–13, D-S3-56).
   - Repo, `package.toml` (all four components declared; `common`, `csv`,
     `toml` hold only a placeholder module until their step if the tool needs
     one), README stub.
   - Base64 standard and URL-safe through one decoder taking the alphabet and
     padding policy; decode row `[InvalidBase64(U64), InvalidLength]`, index a
     0-based UTF-8 byte offset. Scanning left to right, `InvalidBase64(index)`
     for: the first out-of-alphabet byte; a misplaced `=` (partial padding such
     as `QQ=` at the `=`); data after complete padding at its first character;
     excess padding at the first `=` that cannot be there; nonzero trailing
     bits at the last data character of a two- or three-character final group.
     Only a clean scan checks `InvalidLength` (length ≡ 1 mod 4; a
     one-character final group is always `InvalidLength`).
   - Hex: lowercase out, either case in; `InvalidHex(index)` before
     `OddLength`; a shared 256-entry lookup-table builder and byte-index scan.
   - Expects: RFC 4648 §10 vectors both ways (base16 vectors lowercased for
     encode, decoded as written); every rejection and precedence; `QUJDR` and
     `QUJDA` both `InvalidLength`.
2. **`common`** (D-S3-20, D-S3-32, D-S3-44.1, D-S3-44.7, D-S3-48, D-S3-51,
   D-S3-52, D-S3-54.6, D-S3-54.10, D-S3-55.8–10, D-S3-56).
   - `EncodingPath`: `Segment`; `mismatch`; `mismatch_at` generic over a state
     with `key_path`; generic `run` (decode) and `encode_run` (encode) entry
     points; `expected` phrase constants.
   - `EncodingNumber`: sign plus `U128` magnitude accumulator with overflow
     detection; narrowing into every width via the builtin `to_*_try` passed
     in; float parts to `F64`/`F32` (±infinity past range) and `Dec` (exponent
     applied, 18 fractional digits cut toward zero, whole part past range
     refused); shortest spelling for `F64` and `F32` (`1.0`, `-0.0`, `1e300`,
     `1e-7`) with a special-spellings record.
   - `EncodingDate`: the records, calendar and range checks with year and
     offset limits as parameters (time subfields ≤ 999, second 60 refused,
     `24:00` refused), fraction cutting and trimming (at most 9 digits,
     omitted when zero), a fixed-width digit cursor, zero-padded writing, `Z`
     for offset 0.
   - `EncodingText`: one-BOM skip (column 1 after it); UTF-8 position tracker
     with a line-break predicate; `Syntax` constructor; `line L, column C: `
     prefix; UTF-8 byte-order compare and sorted union.
   - Expects: February 29 across century years, month lengths, fraction
     edges, both range-limit sets; narrowing at each width's bounds including
     `U128` max and `I128` min; `Dec` with 19 and 20 fractional digits,
     `1e-30`, `1.5e-20`, largest and one-past-largest whole part; `1e400` and
     `1e39`-into-`F32` as infinity; golden spellings under both special tables;
     columns after astral characters, CRLF and lone CR under each predicate;
     byte-order compare against code-point order.
3. **CSV moved in** (D-S3-1, D-S3-4, D-S3-5, D-S3-17, D-S3-20, D-S3-39,
   D-S3-45, D-S3-49, D-S3-50, D-S3-54.2–4, .12, D-S3-55.3–6, .8–9, .11, .14–15,
   .18, D-S3-56).
   - Copy the playground modules into `components/csv`: one public `Csv` with
     `Csv.Table`, `Row`, `Dialect` (`csv`, `tsv`), `Err`, `EncodeErr`,
     `Segment`, `Parseable`, `Encodable`; format types nested; `Tsv` gone; the
     Bool workaround removed. `parse`, `parse_with`, `to_str`, `to_str_with`,
     `table`, `table_with` (was `parse_with_headers`), `decode`, `decode_with`,
     `encode`, `encode_with`, `encode_columns`, `encode_columns_with`,
     `err_to_str`.
   - Scanning through `EncodingText` (CSV's predicate: LF, CRLF, lone CR);
     paths and `run`/`encode_run` through `EncodingPath`; numbers through
     `EncodingNumber`.
   - Errors: `Csv.Err` (`Syntax`, `RaggedRow({ line, width, found })`,
     `MissingHeader`, `DuplicateHeader`; 1-based lines, code-point columns)
     with D-S3-54.4's quote-error positions, and `err_to_str`.
     `decode`/`decode_with` stop at the first error: `Parse`,
     `Mismatch({ path: [Index(record), Key(column)], expected })` (records
     from 0 after the header; blank, comment and multi-line cells per
     D-S3-54.3) or `MissingRequiredField(Str)`.
   - Cells: XML Schema numbers and booleans (D-S3-50) in `CsvCell`; `Dec` by
     D-S3-48's truncation; out-of-range floats to ±infinity; integer text into
     float fields rounds; an empty cell into a `?:` or `Try` field is absent;
     `Try` fields decode.
   - Writing: header as the byte-sorted union of all records' fields, absent
     fields as empty cells placed by name (D-S3-45); floats by the shared
     spelling with `INF`/`-INF`/`NaN`; `Dec` as `Dec.to_str`; `true`/`false`.
     `encode`, `encode_with` → `Try(Str, Csv.EncodeErr)` with
     `InvalidDate`/`InvalidTime`/`InvalidOffset({ path, offset })` declared now
     (produced from step 6). `encode_columns`, `encode_columns_with` →
     `[DuplicateColumn(Str), UnknownColumn(Str), MissingColumn(Str),
     Encode(Csv.EncodeErr)]`, checked in that order.
   - Skip one leading U+FEFF on every parse path.
   - Port the 156 assertions and `Stress.roc`, rewritten for this model. If the
     cross-package miscompile from `e2b81982` appears, record a minimal
     reproduction and the workaround chosen in Implementation notes and
     continue.
   - Expects: `Bool` encoding and each accepted boolean; each XML Schema number
     form and the refused ones (`1_000`, `0x10`, lowercase `nan`, `+5` accepted
     for integers, grouping); mixed-presence optional fields, including a first
     record without the field, and back through `decode`; an empty cell into a
     `Try` field; `encode_columns` errors and their precedence; duplicate
     headers and two empty names; columns after astral characters and after
     CRLF; a record's first bad cell with its path; quote error positions;
     `RaggedRow` width.
   - Golden output (D-S3-17): quoting (delimiter, quote, line break), `1.0`,
     `-0.0`, `1e300`, an `F32`, `INF`, `-INF`, `NaN`, a `Dec`, `true`/`false`,
     the header union, `Crlf` newline.
4. **TOML reading** (D-S3-7, D-S3-8, D-S3-11, D-S3-20, D-S3-32, D-S3-34,
   D-S3-35, D-S3-37.5, D-S3-38, D-S3-43, D-S3-44.1–2, .4, .6, .8, D-S3-48,
   D-S3-54.1, D-S3-55.1–2, .4, .10, D-S3-56).
   - Lexer and parser for 1.1.0 into nominal `Toml.Value`, scanning through
     `EncodingText` (TOML's predicate: LF, CRLF). Its `is_eq`: entries sorted by
     key (`EncodingText`'s byte compare) then compared; arrays in order; floats
     by `F64` with NaN equal. `Toml.Float` keeps the spelling, with `to_f64`
     (±infinity past range), `to_dec` (D-S3-48 via `EncodingNumber`),
     `float_from_f64`, `float_from_dec` and `is_eq`.
   - Offsets within ±23:59 and years 0–9999 (`EncodingDate` limits), `I64`
     integers via `EncodingNumber`, fraction digits past nanoseconds cut off,
     one depth of 128 over all nesting, duplicates found with a `Dict`, line
     breaks in multi-line strings read as `\n`, leading U+FEFF skipped, CRLF
     accepted.
   - `Toml.Err` per D-S3-55 (`Syntax`, `DuplicateKey({ line, column, path })`,
     `OutOfRange` with source text, `TooDeep({ line, column })`; 1-based lines,
     code-point columns, first error only), `Toml.parse`, `Toml.err_to_str`,
     `Toml.Segment`, `Toml.path`.
   - Corpus: `toml-lang/toml-test` checked in under
     `tests/toml-conformance/corpus` after adding `.gitattributes`
     (`tests/toml-conformance/corpus/** -text`, `tests/toml-edit/cases/** -text`);
     README with commit and case counts. `tests/toml-conformance` runs the
     1.1.0 valid and invalid lists and the 1.0.0 valid list through `parse`; a
     hand-written JSON reader (test code) loads expected output, converted to
     `Value` and compared by its equality; invalid files must be `Err`.
   - Expects:
     - a known CRLF corpus file contains `\r\n`;
     - each error kind and position;
     - depth 128 accepted and 129 refused (`TooDeep`) via arrays, inline
       tables, headers and dotted keys;
     - `I64` bounds (`-9223372036854775808` ok, `9223372036854775808` and
       `0x8000000000000000` refused as `OutOfRange` with that text); offset
       overflow;
     - BOM; CRLF; LF and CRLF multi-line strings equal; an escaped `\r` kept;
       multi-line strings with `"""`, a trailing `"`, a leading newline;
     - `[[x]]` after a static array `x`; a duplicate inside the second
       `[[bin]]` carrying `Index` in its `path`;
     - equality with duplicate keys and nested NaN; a reversed 10,000-key
       compare within a time budget;
     - `to_dec` with 19 and 20 fractional digits, `1e-30`, `1.5e-20`, the
       largest and one-past-largest whole part; `to_f64` of `1e400`.
5. **Typed TOML and writing** (D-S3-9, D-S3-15, D-S3-16, D-S3-17, D-S3-24,
   D-S3-28, D-S3-33, D-S3-37.2–4, .6, .8, D-S3-38, D-S3-43, D-S3-44.4, .6–7,
   .9, .12, D-S3-48, D-S3-54.6, .10–12, D-S3-55.4–5, .7, .10, .16, .18,
   D-S3-56).
   - TOML format over `Value`: `parse_*` for every scalar width
     (`EncodingNumber` narrowing; integers into floats when exact; floats into
     integers are `Mismatch`), `parse_dec` from `Toml.Float`'s spelling with
     D-S3-48 truncation, records, lists, `Dict` with `Str` keys, `Try` fields
     (decode only), the date contract with TOML's RFC 3339 grammar.
     `Mismatch({ path, expected })` via `EncodingPath`; `MissingRequiredField`
     is the compiler's `Str`.
   - `Toml.Parseable`, `Toml.Encodable`; `Toml.decode` (`parse` then
     `decode_value`; its `where` clause repeats `Parse(Toml.Err)`) and
     `Toml.decode_value`. The four date types (with `is_eq`) and `Toml.Value`
     get hand-written codecs by the date contract's pattern; `Value`'s are tied
     to TOML's format. If `Encodable` does not compile, keep explicit `where`
     clauses and record it.
   - Writer: D-S3-15 layout; `Dict` keys sorted by bytes; non-empty bare keys
     (`""` quoted); D-S3-37.4 escapes with uppercase hex digits and `\r` always
     escaped; kept float spellings, else `EncodingNumber`'s spelling with
     `inf`/`-inf`/`nan` (`F32` by its own; `encode_value` keeps the `F32`
     spelling in `Toml.Float`); fractions via `EncodingDate`;
     `Toml.Write` (`V1_1` = `\e`/`\xHH`, seconds omitted when seconds and
     fraction are zero). `to_str`, `to_str_with`, `encode`, `encode_with`,
     `encode_value`, all `Toml.EncodeErr` per D-S3-55 (`path` fields,
     `InvalidOffset({ path, offset })`, `InvalidTime` for subfields over 999);
     depth checked for `Value` trees and typed values. No `encode_null`, no
     `encode_tag`, only `encode_key_str`.
   - A test-only strict 1.0 checker in `tests/toml-conformance`, tested
     against both 1.0.0 lists (valid accepted, invalid refused) and applied to
     every default-mode output.
   - Expects:
     - golden output for both modes: every escape class (uppercase hex),
       `1.0`, `-0.0`, `1e300`, `1e-7`, `inf`, `-inf`, `nan`, an `F32` through
       `encode` and through `encode_value` then `to_str`, kept spellings,
       fraction digits, an empty key, sorted `Dict`;
     - `decode(encode(v)) == v` in both write modes over every supported type
       except `Try`, with `Dec` past 17 digits, and a `Dec` through
       `encode_value`/`decode_value`;
     - the measure-inside-containers rule; every `EncodeErr`; tags and paths of
       errors, not their `expected` text (D-S3-54.11).
   - Conformance suite: `parse(to_str(v)) == v` for every valid file in both
     modes.
6. **Dates across packages** (D-S3-22, D-S3-25, D-S3-36, D-S3-37.1, .9,
   D-S3-39, D-S3-51, D-S3-55.4–5, .7).
   - trantor-encoding, CSV's eight date methods by XML Schema 1.1 (D-S3-51) in
     `CsvDate` over `EncodingDate` (years any `I32`, offsets ±14:00):
     timezone presence decides local versus offset fields; extended years
     written outside 0–9999; offsets beyond ±14:00 are
     `InvalidOffset({ path, offset })`; `InvalidDate`/`InvalidTime` for invalid
     fields; parse fails with `Mismatch`; `24:00:00` refused. Expects: each
     form into each field kind, the refused variants, extended years both
     ways, a fraction longer than nanoseconds, `-00:00`, a year-10000
     `PlainDate` written, `Toml.LocalDate` fields decoded and encoded through
     CSV records; golden output for each date kind.
   - trantor-temporal:
     - `PlainDate`/`PlainTime` codecs over the contract (`Mismatch({ path,
       expected })`), with no import of trantor-encoding; encode writes ISO
       fields and ignores `cal`;
     - `plain_date_from_fields`, `plain_time_from_fields`;
     - `zoned_from_offset!`, `to_offset_datetime!` (converting the instant to
       the rounded offset);
     - trantor-encoding in `[dev-deps]`, and a suite decoding and encoding
       `PlainDate`/`PlainTime` fields through TOML inside records, lists and
       dicts and through CSV in records;
     - expects for a negative offset under an hour, an IANA zone losing its
       name, and a local-mean-time offset keeping the instant;
     - README: JSON is a compile error; zone name and calendar are dropped.
   - trantor-hash: `HashFormat`'s four encode methods; golden expects pinning
     the layout; existing golden hashes unchanged; a README line.
7. **TOML editing** (D-S3-10, D-S3-17, D-S3-20, D-S3-29, D-S3-30, D-S3-31,
   D-S3-34, D-S3-40, D-S3-41, D-S3-42, D-S3-43, D-S3-44.5, D-S3-46, D-S3-47,
   D-S3-53, D-S3-54.7–9, D-S3-55.4, .17).
   - `Toml.Document`: a lossless tree with trivia (whitespace, comments, line
     endings, BOM) and source spellings. `parse_document`, `to_str`,
     `to_value`, `get`, `set`, `set_with` (`Toml.Edit`), `remove`, `append`,
     all `Toml.EditErr` per D-S3-55 (`StyleNotPossible({ path, style })`);
     empty paths per D-S3-54.8, with `set([], table)` as per-key edits under
     the root keeping trivia; depth errors as `Encode(TooDeep)`.
   - Rules:
     - styles `Auto`/`Inline`/`Header`/`Dotted` by D-S3-41 and D-S3-54.7, with
       `StyleNotPossible` and a `[header]` for the direct parent of an inline or
       dotted table;
     - header-created and dotted-created tables take additions only in their
       own form (D-S3-46);
     - kind changes remove and re-add, keeping position where possible
       (D-S3-47); a scalar becoming a table under `Auto` is inline in place; an
       inline table becoming a scalar is replaced in place (D-S3-55.17);
     - removal spans (D-S3-31) by path prefix (D-S3-40);
     - new sections after their family's last section, else at the end
       (D-S3-53);
     - `append` layouts; a non-table appended to `[[x]]` is `NotATable(path)`;
       edits following existing layout, with `version` governing new
       constructs (D-S3-42).
   - Conformance suite: every valid file through `parse_document`/`to_str`
     byte-identical, and `to_value` equal to `parse`'s.
   - `tests/toml-edit`: every case checks the byte snapshot, that `after.toml`
     parses, and that `get(path)` equals the value set (`NotFound` after a
     removal). One case each:
     - replace a value keeping its trailing comment; add a key to a header, a
       dotted and an inline table;
     - `Auto` producing inline in a `[deps]`-style table, header elsewhere,
       dotted under a dotted parent, header under a Cargo-style mixed parent and
       under a table with no keys, `[[x]]` for an array of tables, inline for an
       array of tables under a dotted parent; forced `Inline`/`Header`/`Dotted`;
       `StyleNotPossible`; `trantor add`'s first dependency under `Auto` and
       `Inline`; implicit parents (`components.x.kind` with no `[components]`);
       `[bin.sub]` inside a `[[bin]]` element;
     - a key added under `[p.x.y]`'s implied `p.x` (new `[p.x]` section);
       `Dotted` there refused; a `Header` under a dotted table refused;
       `Inline` replacing a header-created table;
     - kind changes: table → scalar and scalar → table under `Auto`, `Header`,
       `Dotted`, with comments and with sub-sections; an inline table → scalar
       in place;
     - section placement: a new `[components.b]` between `[components.a]` and
       `[wiring]`, a new top-level section, a nested family, a family without
       blank lines;
     - remove a key with its comment block, a multi-line value, an
       inline-table entry, a header table with its sub-sections, a scattered
       dotted table, an implicit table, sub-sections before their parent, one
       `[[x]]` entry, the last key of each kind of table;
     - append to single-line, multi-line and `[[x]]` arrays, inside a
       multi-line inline table under `V1_0`, and a scalar to `[[x]]`
       (`NotATable`);
     - `Index` paths into `[[bin]]`; each edit function with an empty path,
       and `set([], table)` keeping comments; CRLF file (including a
       multi-line string set into it); BOM file; `set_with` `V1_1`; a typed
       record through `encode_value` and `set`.
8. **Documentation.**
   - trantor-encoding README in trantor-temporal's style: setup, an example per
     module, each module's types and signatures, and:
     - the TOML version policy and byte-stability (D-S3-17), with message text
       excluded (D-S3-54.11); hex as RFC 4648 base16 lowercased;
     - `MissingRequiredField` naming only the field (D-S3-28); `Try` fields
       decode only (D-S3-54.12);
     - building a `Value` with `Toml.float_from_f64` (D-S3-54.13);
       `Toml.Float` and `Dec` exactness and truncation (D-S3-38, D-S3-48);
       out-of-range float literals as infinity (D-S3-55.10);
     - the `Toml.Edit` annotation when stored; dotted and implicit tables
       vanishing with their last key;
     - `Toml`'s date types working with CSV too (D-S3-55.7);
     - CSV: XML Schema cell forms (D-S3-50, D-S3-51), `24:00:00` refused as a
       departure, integer text into float fields rounding (D-S3-55.11), the
       header union and `UnknownColumn` for all-absent optional fields
       (D-S3-45), finding a record by index with `Csv.table` (D-S3-54.3), the
       Excel BOM note;
     - second 60, known limits.
   - `tests/readme` runs its examples.
   - trantor: D-S1-11 step 3 points at the package; implementation notes below.

## Implementation notes

*(Filled in as work lands: commits, measurements, findings that contradicted or
went beyond the log, and the choice made for each under the working rule.)*

### Step 1

trantor-encoding `dbfb810`. `trantor test .`: PASS, no warnings; 319 expects
run, 85 the package's own (Base64 53, Hex 26, Base64Lookup 6); 0.36 s wall.
`Base64.roc` 302 lines, `Hex.roc` 110, `Base64Lookup.roc` 49.

- **`package.toml` declares only `base64`,** not all four components: the step
  was scoped to declare what exists, and each later step adds its component.
- **The shared table and scan live in a third module, `Base64Lookup`,** exported
  by the component and not the package (trantor-hash's `HashFormat` pattern).
  D-S3-56 has Hex share Base64's table builder and scan while `base64` "imports
  nothing"; one module cannot share with another without an import, so "imports
  nothing" is read as no imports outside the component.
- **The padding policy governs output only.** D-S3-2 makes padding optional on
  decode for both alphabets, so the decoder takes the alphabet record
  (`digits`, `lookup`, `is_padded`) and ignores `is_padded`.
- **Undefined cases, chosen by the left-to-right rule:** `=` after a group of 0
  or 1 characters (`=`, `Z=`, `Zm9v=`) is a misplaced `=` at that index, not
  `InvalidLength`; unused bits are checked when the final group is known (at
  the first `=`, or at the end of a clean scan), so `Zh=` is `InvalidBase64(1)`
  but `Zh!` is `InvalidBase64(2)`; incomplete padding followed by anything
  (`QQ=A`, `QQ=!`) reports the first `=`.
- **Measured:** an error-row alias on a scan step's annotation must name the
  whole row (`[InvalidHex(U64), OddLength]`); the narrower `[InvalidHex(U64)]`
  fails the caller's `?` as a type mismatch. `trantor test` then reported "0
  compiler errors" with the affected expects as runtime crashes, so a failing
  count needs the full output read.

### Step 2

trantor-encoding `bf07259`. `trantor test .`: PASS, no warnings (each module
also clean under `roc test --no-cache`); 535 expects run, 301 the package's own,
216 of them `common`'s (EncodingNumber 95, EncodingDate 77, EncodingText 37,
EncodingPath 7); 0.5 s wall. Each module's expects were checked by mutation
(a changed expectation fails). Lines: EncodingNumber 358, EncodingDate 357,
EncodingPath 225, EncodingText 153; the first two pass ~300 only through their
expects (code about 215 and 230), kept in-module as the step lists them.

- **Measured: the builtin float parsers refuse, not round, past the range.**
  `F64.from_str("1e400")`, `F64.from_str("0.17976931348623159e309")` and
  `F32.from_str("1e39")` are `Err`; `1e-400` gives `0` and `-1e-400` `-0`. So
  `EncodingNumber` normalizes the parts to significant digits and a
  decimal-point position, answers zero or infinity outright beyond ±400, gives
  the builtin only canonical text (`0.123e-5`), and reads a refusal as
  infinity (D-S3-55.10). Exponents past `U64` are capped at `U64`'s largest.
  Grammar-level quirks of `from_str` (D-S3-50) cannot reach it.
- **`Dec` is built from attos,** not `Dec.from_str`: digits to the 18th
  fractional place accumulate into `U128`, narrow to `I128` and go through
  `Dec.from_attos`. "Whole part past range refused" is read as "value past
  `Dec`'s range": `170141183460469231731.687303715884105727` is `Dec.highest`,
  `-…731.687303715884105728` `Dec.lowest`, and both `…732` and `…731.9` are
  `OutOfRange` (the second has an in-range whole part but no representation).
- **Digit values, not characters,** cross into `EncodingNumber` (`0`–`9`,
  `0`–`15` for hex, TOML's underscores already dropped); `FloatParts` is
  `{ is_negative, whole, fraction, exponent : { is_negative, digits } }` so the
  exponent cap lives in `common`.
- **Narrowing is two functions,** `to_unsigned` taking a `U128 -> Try(n, …)`
  builtin and `to_signed` taking an `I128 -> Try(n, …)` one: `U128.highest`
  does not fit the prototype's single `I128` funnel. Both answer
  `[OutOfRange]` instead of taking the state and building `Mismatch` as the
  prototype's `narrow` did, which keeps paths out of the number module; the
  formats map the tag (`Mismatch` in typed decode, `Toml.Err.OutOfRange` in
  parsing).
- **`F64.to_str` spells `1e15` as `1000000000000000` but `1e16` as `1e16`,**
  so the `.0` fix is needed up to 1e15 (`1000000000000000.0`); `F32`'s
  `16777216` becomes `16777216.0`. Spellings never contain `e+`.
- **`EncodingDate`'s parameters are one `Limits` record**
  (`min_year`, `max_year`, `max_offset_minutes`); the expects define TOML's
  (0–9999, 1439) and CSV's (`I32` range, 840). Reading: `offset_from_clock`
  (minutes past 59 or a total past the limit are `OutOfRange`; `-00:00` is 0),
  `fraction_from_digits`, and a `Cursor` (`fixed_digits`, `digit_run`,
  `expect_byte`, `is_at_end`) failing with `Unexpected(index)`. Writing:
  `pad`, `date_text` (at least four year digits, `-` for negative years, so
  TOML's range and CSV's extended years both write through it), `time_text`,
  `fraction_text` (including the `.`, empty for zero) and `offset_text`. The
  field separators are the same in RFC 3339 and XML Schema; TOML's `V1_1`
  seconds omission is left to TOML, built from `pad` and `fraction_text`.
- **Positions are computed from a byte index when an error is reported**
  (`EncodingText.position_at(bytes, index, ends_line)`) rather than tracked per
  byte while scanning: both formats report only the first error, so scanners
  carry an index and pay for lines and columns once. The predicate says whether
  the byte at an index ends its line; in CRLF the CR is a column of the old
  line. `skip_bom` works on bytes. `sorted_union` takes every record's names as
  `List(List(Str))`.
- **`expected` phrases cover the scalar types and the four date kinds**
  (`EncodingPath.expected_u8`, … `expected_offset_datetime`); phrases for
  tables, arrays and cells stay in each format.
- **The test format lives at the top level of `EncodingPath.roc`** as private
  nominal types (`ProbeFormat`, `ProbeState`, `ProbeEncoder`), not in a test
  component: measured to compile and run, and it keeps component exports to the
  four modules. It proves `run` (a record decoded, a `Mismatch` with
  `[Index(4), Key("age")]`, `MissingRequiredField`) and `encode_run` (a nested
  record, an error variable carrying `[Key("b"), Key("c")]` out).
- **Measured compiler behaviour:** an open-row alias (`X : [A, ..]`) is
  refused ("open ext not allowed in type declaration"), so the probe spells its
  rows out; a trailing `? |_| Tag` on a returned value is a warning, so
  `map_err` is used there; a top-level constant containing `match` is
  evaluated at compile time and warns "unused branch" for arms it did not take
  (seen in a probe, avoided in the modules).

### Step 3

trantor-encoding `6792867`. `trantor test .`: PASS, no warnings (each csv
module also clean under `roc test --no-cache`); 781 expects run, 547 the
package's own, 246 of them `csv`'s (CsvTestRows 94, CsvCell 40, CsvTestEncode
34, CsvTestTable 31, CsvTestDecode 31, CsvEmit 10, CsvStress 6); 1.2 s wall.
Mutations fail as expected (a golden `INF` changed, a decode path, a `Dec`
value). Lines: Csv 496 (about 250 of code; the rest docs and the one-line
format methods), CsvParse 254, CsvCell 204 (with its expects), CsvEmit 89.

- **Layout:** `Csv` (surface, format and states), `CsvParse` (scanner and every
  `Csv.Err`), `CsvEmit` (writer), `CsvCell` (XML Schema grammar and spellings
  over `EncodingNumber`), and the expects in `CsvTestRows`, `CsvTestTable`,
  `CsvTestDecode`, `CsvTestEncode`, `CsvStress`, all component exports (trantor
  test refuses expects in a module no component exports). No `CsvDate` yet.
- **`Dialect`, `Row`, `Table`, `Format`, `DecodeState`, `EncodeState` nest in
  `Csv`.** `CsvParse` and `CsvEmit` cannot import `Csv` (it imports them), so
  each takes a structural `Syntax` record and `Csv` converts the dialect.
  Measured: a nominal whose backing is another module's alias does not accept a
  record literal, so the backing is written out in `Csv`. An alias to a nominal
  in another module does carry its associated values (`Nd.Dialect.csv` and
  `{ ..Nd.Dialect.csv, … }` work) and was not needed.
- **`Csv.Err : CsvParse.Err`,** the structural union written once where it is
  produced; `Csv.Segment : EncodingPath.Segment` likewise.
- **`Parseable` and `Encodable` both compile and are used** by every typed
  signature (`a.Parseable(errs)`/`a.Encodable(err)` nested in `Csv`, referenced
  as `a.Csv.Parseable(…)` from the module's top level). D-S3-55.18's open
  question is closed for CSV.
- **No cross-package miscompile on `10e922df`.** A scratch copy of the package
  with an app suite (`tests/xpkg`, not committed) decoded TSV with `Try` and
  `?:` fields, read `Csv.Dialect.tsv.delimiter`, encoded a header union with
  `U64.highest`, `-0.0` and `INF`, and printed a `Mismatch` path and an
  `err_to_str`, all correct through the composed platform.
- **Empty cells, measured:** the derive never tells a format whether a field is
  optional (`FieldNames` carries names only; `parse_null` is not consulted for
  `?:` or `Try` fields, a JSON `null` into either is `InvalidJson`), and
  `Continue(state)` from `parse_record_field` skips a column. So an empty cell
  is skipped, which makes it absent for `?:` and `Try` fields (D-S3-45,
  D-S3-55.14); a required field then answers `MissingRequiredField(name)`, and
  when `name` is a header whose cell was skipped, `decode` offers that column's
  empty cells for this and every later record and reads the record again. A
  required `Str` gets `""`, a required number a `Mismatch` at its path; each
  column costs at most one retry for the whole document (200,000 records with
  an empty required cell each decode in about a second).
- **Undefined cases, chosen:** a record-typed field is `Mismatch` at its column
  on decode (a CSV cell is a scalar); on encode the protocol shares one state
  type and `EncodeErr` has no tag for it, so a nested record writes an empty
  cell — a known limit for step 8's README. `encode` and `encode_columns` of no
  values write `""`; `encode_columns([], ["a"])` is `UnknownColumn("a")`.
  `DuplicateHeader`'s column is the field's first character after `trim`'s
  skipped spaces (its opening quote when quoted). A header under
  `skip_blank_lines`/comments is the first record that is not skipped.
- **`encode_columns` precedence with encode errors (for step 6):** names come
  from the records that encoded, so a record failing with a date error drops
  its names from the `UnknownColumn`/`MissingColumn` checks. Nothing fails to
  encode until step 6's date methods; if they return `Err`, a column only that
  record has would read as unknown. Step 6 should keep D-S3-55.15 exact, e.g.
  by recording the first date error in the encode state and returning it after
  the record's names are known.
- **Grammar details:** `+INF` reads, `+NaN`/`-NaN` do not; `-0` reads into
  unsigned widths as 0 (`EncodingNumber.to_unsigned`); `Dec` takes the float
  grammar without specials; the error `expected` texts are new (message text,
  unpromised). `CsvParse` keeps the playground's record loop on plain
  parameters (its measured 4.8 s against 0.15 s) and computes positions only
  for the error reported.
- **Port of the playground's 156 assertions:** rewritten rather than copied —
  message assertions check tags and positions; `Tsv` expects use
  `Csv.Dialect.tsv`; accumulated `BadCells` became first-error paths (records
  from 0); `Parse.pos`/`strip_bom` unit expects are covered by
  `EncodingText`'s; case-insensitive `TRUE`/`FALSE` became a refusal.
  `Stress.roc` keeps its four scanner cases and adds typed decode (empty
  required cells) and encode over 20,000 records.
- **Measured compiler behaviour:** uppercase and non-ASCII record field names
  do not parse, so byte order against derive order is tested with `a1`, `a_b`,
  `ab`; `List.join_with` and `Try.and_then` do not exist (`Str.join_with`,
  `map_ok` with `??`); passing a tag constructor as a function (`map_err(Encode)`)
  is a type error, a lambda works; match guards (`Ok(b) if …`) and `|`
  alternatives in patterns compile; a record literal with `True`/`False` fields
  needs a `Bool` annotation to reach `encode_bool` (unannotated it is a tag
  union and asks for `encode_tag`).
