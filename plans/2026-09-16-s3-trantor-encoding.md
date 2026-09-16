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

### Step 4

trantor-encoding `cf9a91d` (reading) and `809ac65` (corpus and suite).
`trantor test .`: PASS, no warnings; 1025 expects run, 791 the package's own,
244 of them `toml`'s (TomlTestErrors 44, TomlNumber 40, TomlTestParse 36,
TomlTestValue 34, TomlDate 31, TomlString 27, TomlLex 23, TomlStress 9).
Conformance (toml-test `ff49d109`): 1.1.0 valid 218/218, 1.1.0 invalid
494/494, 1.0.0 valid 208/208, all on the first run; no case contradicted the
log. The reversed 10,000-key compare takes 15 ms in the built app. Mutations
fail as expected: depth limit 129, NaN inequality, lost quotes before a
closing delimiter, a missing `Index` in `[[x]]` paths, a clipped `OutOfRange`
text, exponents dropped in `to_dec`, dotted keys into implicit tables (an
expect catches it), a changed expected JSON (the suite prints `FAIL` lines and
fails). Lines: TomlString 347 (316 before its expects), TomlTree 266,
TomlNumber 247, TomlDate 231, TomlLex 210, TomlParse 167; the app 331 (test
code).

- **Layout:** `Toml` (surface), `TomlParse` (expressions, arrays, inline
  tables, `Toml.Err`), `TomlTree` (table assembly), `TomlLex` (trivia, keys,
  scalar dispatch), `TomlString`, `TomlNumber`, `TomlDate` (grammars),
  `TomlValue` (`Value`, `Float`), `TomlProblem` (byte-indexed scan errors and
  the depth limit), and the expects in `TomlTestParse`, `TomlTestErrors`,
  `TomlTestValue`, `TomlStress`.
- **`Value` and `Float` live in `TomlValue` and `Toml` aliases them**
  (`Toml.Value : TomlValue.Value`): the parser builds values and `Toml`
  imports the parser. Measured: a recursive `Value := [...]` nested in a
  module type compiles; through the alias, bare tags construct it under an
  annotation, `==` uses its `is_eq`, and `Toml.Float` values take
  `.to_f64()`/`.to_dec()`. `Toml.Value.Table(...)` is refused ("an alias, not
  a nominal type"). `Float :: { spelling, value : F64 }` is opaque; the `F64`
  is computed once, `to_dec` re-reads the spelling through `TomlNumber`
  (underscores included). The parser's constructor is
  `TomlValue.float_read`, outside `Toml`.
- **`Toml.Date`, `Toml.Time`, `Toml.Offset` are added now** (aliases of
  `EncodingDate`'s records, D-S3-23), since `Value`'s payloads name them.
  `EncodingDate`'s TOML limits are a private test constant, so `TomlDate`
  defines its own (years 0–9999, ±23:59).
- **Tables are assembled in a flat slot list** (`TomlTree`), not a nested
  `Value`: a slot per table or array of tables, children by index, a `Dict`
  per table for duplicates (D-S3-44.1), converted to `Value` at the end. Each
  table records its origin, which is what TOML's reopening rules turn on:
  `Implicit` (named on the way to a header; a header may define it once),
  `Header` (only deeper headers reach in), `Dotted` (more dotted keys and
  deeper headers extend it; a header never defines it). Inline tables and
  array values are finished `Value`s. An inline table is read through a
  fresh tree rooted at its own level and path, so dotted keys inside it follow
  the same rules. Measured: `List.set` answers `Try`, and a slot read with
  `get` then written back copies its lists, so slots are taken out with
  `List.replace` (a `Taken` placeholder) while they change. 10,000 keys in a
  table, 10,000 `[[x]]` elements, a 10,000-element array and 200 KB strings
  of each kind parse inside `TomlStress`.
- **Depth, made concrete:** the root table is level 1; a table or array that
  would be level 129 is `TooDeep` at the bracket or key segment opening it.
  An `[[x]]` costs two levels (the array and its element). So 127 nested
  arrays, 127 nested inline tables, a 127-segment header and a dotted key
  creating 127 tables are accepted, one more refused, and a 126-segment
  header then `c = [[]]` is refused at the inner bracket.
- **Undefined error cases, chosen:**
  - `DuplicateKey` covers every conflicting definition, not only a key set
    twice: a header reopening a defined table, a dotted key into a header,
    implicit or inline table, `[[x]]` over a static array or a table, a header
    through a scalar. It sits at the conflicting key segment, with the path up
    to it (`Index` inside arrays of tables). The key is resolved before the
    value is read, so a duplicate wins over a malformed value after it.
  - `OutOfRange` holds integers past `I64` and well-formed dates, times and
    offsets outside their fields' ranges (month 13, February 29 of a common
    year, hour 24, minute 60, second 60, an offset past ±23:59); `text` is
    the whole literal, at its first character.
  - An escape naming a surrogate or a value past U+10FFFF is `Syntax` at the
    backslash; an unterminated multi-line string is `Syntax` at its opening
    delimiter (CSV's precedent), a single-line one at the line break.
- **Invalid UTF-8 cannot reach `parse`:** it takes `Str`. The suite counts a
  file `Str.from_utf8` refuses as refused (the nine `invalid/encoding` files).
- **Checks an expect cannot make live in the suite:** the CRLF corpus check
  (`test.sh` reads `valid/newline-crlf.toml`; `git ls-files --eol` shows
  `i/crlf attr/-text`) and the timed compare (a 2-second budget; the expect in
  `TomlTestValue` checks only the result). `tests/lib.sh` is new; its
  `build_app` also fails a build that reports warnings.
- **The expected JSON is read without the parser:** the app's JSON reader
  decodes `\u` escapes and RFC 3339 text itself; floats go through
  `F64.from_str` and `Toml.float_from_f64` (`inf`/`nan` by name).
- **Measured compiler behaviour:** a closed error row does not widen through
  `?` (`? |_| OutOfRange(next)`, as step 1 found); `List.range` does not
  exist (`List.repeat` with `map_with_index`); a type named `Json` in an app
  warns that it shadows a builtin; a tag constructor passed to `map_ok` is a
  type error again (step 3). `trantor test` counts expects by `expect`
  statements, so a `grep '^expect'` that also matches `expected_…` constants
  overcounts.
- **Not done here:** typed decode, writing, `Document`, the four nominal date
  types and their `is_eq` (step 5 onward).

### Step 5

trantor-encoding `eca2c02` (typed TOML and writing) and `ad0c1d2` (conformance
round trips and the strict 1.0 checker). `trantor test .`: PASS, no warnings
(each new module also clean under `roc test --no-cache`); 1124 expects run, 890
the package's own, 99 of them new (TomlTestDecode 40, TomlTestEncodeErr 23,
TomlTestCodecs 15, TomlTestEncode 14, TomlTestRoundTrip 7); 20 s wall.
Conformance: the three parse lists unchanged; `parse(to_str_with(v, mode)) ==
v` in both modes for 1.1.0 valid 218/218 and 1.0.0 valid 208/208, every `V1_0`
text strict; the strict checker accepts 1.0.0 valid 208/208 and refuses 1.0.0
invalid 501/501. Mutations fail as expected: lowercase hex digits, `\x1B` for
`\e`, seconds dropped in `V1_0` (golden expects, and with expects bypassed 28
round trips fail the strict checker), sections before arrays of tables, an
unsorted dict, the depth limit at 129, exactness dropped for integers into
floats, `Dec` through `F64`, `F32` widened, a lost `Index`, duplicate and
offset and date checks removed, a date codec altering its value (8 container
expects), and the checker missing `\xHH`. Lines: TomlFormat 260, Toml 250,
TomlEncode 178, TomlText 132, TomlValue 118, TomlCheck 106, TomlCursor 102,
TomlWrite 81; the suite's app 160, ExpectedJson 235, StrictToml 102.

- **Layout:** `TomlFormat` (the decode format's methods and number rules),
  `TomlCursor` (`DecodeState`, frames and paths), `TomlEncode` (the encode
  format, building a `Value`), `TomlCheck` (`EncodeErr` and every check before
  writing), `TomlText` (escapes, keys, spellings, inline values), `TomlWrite`
  (D-S3-15's sections). `Toml` holds the surface, `Parseable`/`Encodable`, and
  the four date types nested as `Toml.LocalDate := { year, month, day }` and so
  on (records written out, per step 3's finding). `encode` is `encode_value`
  then `to_str_with`, so the two paths cannot disagree.
- **`Value`'s codecs are tied to TOML by method name, not by type.** D-S3-33
  has `Value.parser_for` name TOML's format; `Value` lives in `TomlValue`,
  which the format modules import, and a module cannot import back. So
  `parser_for` requires `parse_toml_value` (closed `Mismatch` row, as the date
  contract) and `encoder_for` requires `encode_toml_value`, which only
  `TomlFormat.Format` and `TomlEncode.Encoder` define. Another format is still
  a compile error, naming the missing method instead of the format. Measured
  inside records, lists, nested records and dict values both ways.
- **`Parseable` and `Encodable` both compile for TOML** and every typed
  signature uses them, as D-S3-55.18 has it; the encoder's methods use the
  closed `Toml.EncodeErr` row (E1c's form), the date types' `encoder_for`
  keeps an error variable.
- **Measured compiler behaviour:**
  - Tuples need `invalid_value : fmt, state -> err` on the format (the
    derived tuple parser calls it); records, lists, dicts and `Try` fields do
    not. `Format.invalid_value` answers `Mismatch` at the cursor. D-S3-28's
    finding that the hook is never selected stays true for missing fields.
  - `{ name }` with a single variable is a block, not a punned record:
    `Toml.encode_value({ raw })` encodes `raw` itself. Tests write
    `{ raw: raw }`; a README note belongs to step 8.
  - A record update producing a nominal (`time : Toml.LocalTime` then
    `time = { ..midnight, second: 60 }`) hangs the compiler in `roc test`, and
    in a module with other expects crashed it with SIGSEGV; full literals are
    used.
  - Type aliases that double nesting (`L64(a) : L32(L32(a))`) take the
    compiler exponential time (L32 4 s, L64 over a minute), so the typed depth
    test spells its 127- and 128-deep `List` types out.
  - Fields of a `::` type are private to its module even through a namespace
    function in another module (`state.value` outside `TomlEncode` is a type
    error); `TomlEncode.result` reads it, and `DecodeState` is `:=` so
    `TomlFormat` can read the cursor `TomlCursor` owns.
  - A nominal gets no derived codecs, so a recursive user type cannot reach
    `TooDeep`; typed depth is reached only through a type nested 128 deep.
- **Choices where the log is silent:**
  - Every sub-table gets its own `[header]`, even one holding only sub-tables
    or arrays of tables (`[inner]` then `[[inner.a]]`); the root writes no
    header and an empty document writes `""`. A non-empty array holding only
    tables is `[[x]]`; an empty array or a mixed one is inline.
  - A string holding `\n` is written `"""` followed by a line feed, so a
    leading line feed survives the one reading drops. `"` is escaped in both
    string forms, so no quote run can close a multi-line string early.
  - Kept float spellings are always written: every spelling a `Float` can
    hold is a TOML float literal valid in 1.0 and 1.1 (parsed literals, or
    `EncodingNumber`/`Dec.to_str` output), so the "else shortest" branch of
    D-S3-38 has no case. `float_from_f32` and `float_from_dec` take their `F64`
    by reading their spelling, as a parsed literal does, so `encode_value`
    then `to_str` then `parse` compares equal.
  - An integer into `F32`/`F64` is exact when converting back gives the same
    integer (`9223372036854775807` into `F64` is `Mismatch`); every `I64` fits
    `Dec`.
  - Checks run in document order and stop at the first: a table's entries in
    order, each key's duplicate check before its value; a datetime's date
    before its time before its offset. `DuplicateKey` holds the second
    occurrence's full path. `TooDeep` holds the path of the table or array
    that would be level 129. `encode_value` checks an embedded `Value` as
    `to_str` would (depth from its own path), so both report the same error.
  - `encode_value` of a non-table is allowed (`Array`, `Integer`); only
    `to_str`, and so `encode`, answer `RootNotATable`.
  - The strict 1.0 checker is `Toml.parse` plus a scan outside strings and
    comments for 1.1's additions (a line break, comment or trailing comma
    directly inside an inline table, `hh:mm` not followed by `:`, and `\e`
    or `\x` in basic strings). The 1.0.0 invalid list is the evidence that
    nothing else separates the versions.
- **Moved to step 6:** the four date types through CSV records (D-S3-55.7).
  CSV has no date methods yet, so a record with a `Toml.LocalDate` field does
  not compile against `Csv.Format`; step 6 adds the methods and the test.
- **Suite:** `tests/lib.sh`'s `build_app` copies a suite's other `*.roc` files
  beside `main.roc`, measured to import as sibling modules (`import
  StrictToml`, `import pf.Toml` inside them); the JSON reader moved into
  `ExpectedJson.roc`.

### Step 6

trantor-encoding `d0a2d90`, trantor-hash `0951c10`, trantor-temporal
`d785f9b`. `trantor test .` passes with no warnings in all three:

- trantor-encoding: 1177 expects run, 943 the package's own (53 new: CsvDate 29,
  CsvTestDate 24); `tests/date-codecs` 4 lines; conformance unchanged; 17 s wall.
- trantor-hash: 262 expects run, 28 its own (5 new); `tests/layout` 12 lines (4
  new), and the 8 existing lines are byte-identical.
- trantor-temporal: 909 expects run, 174 its own (14 new, in `Plain`);
  `tests/formats` 11 lines and `tests/offsets` 8 lines, both new; every
  existing suite unchanged. The full run, sweeps included, takes 5 min wall.

Mutations fail as expected:
- encoding: CSV's offset limit at 15 hours; the no-leading-zero rule for
  5-digit years removed; a later date problem overwriting the first;
  `encode_columns` taking names only from records that wrote cleanly (3
  expects);
- hash: day and month swapped in `write_date`;
- temporal: `parser_for` lifting onto `Hebrew` (7 suite lines); offset
  rounding truncated (the Monrovia line); `floor_div` truncating, and the
  civil-date era adjustment dropped (`Plain` expects).

Lines: Csv 558 (was 496), CsvDate 263 (about 130 before its expects),
CsvTestDate 244; HashFormat 295; Temporal 804 (was 751), Plain 305 (was 196).

- **CSV writing keeps the first problem in the encode state (step 3's open
  issue).** `EncodeState` carries the record index, the column, and a
  `problem : [Clean, Failed(EncodeErr)]`. The date encode methods always
  answer `Ok`: a cell that cannot be written becomes empty and its problem is
  recorded if it is the record's first. `encode_record` threads the problem
  through `EncodeFields`. `encode`/`encode_with` return the first record's
  problem. `encode_columns_with` checks names from every record, then
  columns, then that problem, so D-S3-55.15's order holds for a record that
  cannot be written (expects on `Encode`, `MissingColumn` and `UnknownColumn`
  over the same failing record).
- **`Csv.EncodeErr : CsvDate.EncodeErr`,** written once where it is produced,
  as with `Csv.Err : CsvParse.Err`. CsvDate's `*_text` functions take the
  cell's path and return the whole `EncodeErr`. Checks run date, then time,
  then offset.
- **Grammar choices where D-S3-51 is silent:** a year of more than four digits
  with a leading zero (`02026`) is refused, as XML Schema's `yearFrag` is;
  `-0000` reads as year 0; a year of more than ten digits, or outside `I32`,
  is `Mismatch`; `+hh:mm` without the colon is refused. An empty date cell is
  absent for an optional field. For a required field it is a `Mismatch` at the
  cell, after the step 3 empty-cell retry.
- **The in-component date tests use test types.** `csv` cannot import `toml`,
  so `CsvTestDate` defines four nominals with the contract's codecs. The
  moved sub-test (D-S3-55.7) is `tests/date-codecs`, an app over `pf.Csv` and
  `pf.Toml`: `Toml`'s four date types decode from CSV records, write back,
  round-trip, and report `Mismatch` and `InvalidOffset` with CSV's paths.
- **Measured in apps:** a top-level constant or a `|{}|` function whose
  `match` value is computable at compile time warns ("unused branch" or
  "unconditional condition"), which fails a warning-free build. Suites match
  in helpers that take the value as a parameter.
- **HashFormat's layout:** each field is one `U64` word as an integer leaf is
  (signed sign-extended), in the contract's order: year, month, day; hour
  through nanosecond; a date-time's date words then its time words; an offset
  date-time's `minutes` last. There are no markers, so a date hashes like the
  tuple of its fields. It does not hash like a `{ year, month, day }` record,
  because records hash in alphabetical field order. The Rust model in
  `tests/vectors` computes the four new layout lines, and `HashVectors.roc`
  regenerates unchanged. The README says a `PlainDate`'s calendar is not
  hashed.
- **`to_offset_datetime!` is pure after two reads.** It takes
  `offset_seconds!`, rounds to minutes half away from zero (Monrovia's
  -0:44:30 becomes -00:45, matching the host's own IXDTF rendering), and
  recomputes the ISO wall clock from `epoch_ns!` in Roc (`Plain.wall_clock_at`,
  Hinnant's civil-from-days with a floor division). The alternative is the
  host's `with_time_zone!`, which answers `Try`, and D-S3-25's signature has
  none. It is a `ZonedDateTime` method (`z.to_offset_datetime!()`), with the
  other readers. The record types are `Plain.Date`, `Plain.Time` and a new
  `Plain.Offset`.
- **`zoned_from_offset!`** builds the zone id `±hh:mm` and calls
  `zdt_from_wall_clock!` on `Iso` with `Reject`, since a fixed offset has
  nothing to resolve. An offset of 24:00 or more is the host's `OutOfRange`.
  Measured: an IANA zone comes back as `[-04:00]` with the instant kept, and
  New York's 1850 LMT (-17762 s) becomes 12:00:02 at -04:56 with the instant
  kept.
- **The codecs decode onto `Iso`** through `PlainDate.lift` and
  `PlainTime.new`, and encode `rec()`, dropping the calendar. Measured: a
  `Toml.LocalDate` passes to `plain_date_from_fields` and is refused by
  `plain_date` (type mismatch), as D-S3-37.1 has it. `tests/formats` shows the
  first; the README states both.
- **Temporal's checks are suites, not expects:** the conversions call the
  host. Only `Plain`'s pure arithmetic has expects (the package had none
  outside `Strftime`). trantor-encoding's expects now also run in temporal's
  composed world (909 in total).
- **Not done here:** CSV's README section on dates and `24:00:00` (step 8).

### Step 7a

trantor-encoding `2bcd960`: the lossless document and read-only access (the
edit operations are step 7b). `trantor test .`: PASS, no warnings (each new
module also clean under `roc test --no-cache`); 1255 expects run, 1021 the
package's own, 78 of them new (TomlTestDocument 41, TomlTestIndex 33,
TomlStress 4); 30 s wall (was 17 s; the corpus app runs in 0.17 s and the
module expects in under 4 s, so the rest is building). Conformance: the
earlier lists unchanged; `parse_document` then `to_str` byte-identical with
`to_value == parse` for 1.1.0 valid 218/218 and 1.0.0 valid 208/208, and
1.1.0 invalid 494/494 refused with `parse`'s exact error. Mutations fail as
expected: key dots dropped (4 expects, and 13 corpus files in the suite run
directly), an array's closing trivia lost, the BOM lost, line endings merged
into trailing text, inline entries without their route position, implicit
tables reported as header tables, the root not reported, and wrong `NotFound`
and `NotATable` paths. Lines: TomlParse 223 (was 167), TomlIndex 184,
TomlSyntax 116, TomlDocument 83, Toml 271 (was 250).

- **One pass builds both.** `TomlParse.read` returns the `TomlTree` and a
  `TomlSyntax.File`; `Toml.parse` takes the tree, `parse_document` the file.
  The grammar and every check stay where they were, so the first error and
  its position are `parse`'s by construction (a two-pass syntax-then-tree
  reader would reorder errors: `a.b.? = 1` with `a` a duplicate is `Syntax`
  today). `TomlLex.KeyPart` gained `next`, the byte after the segment.
  `Toml.parse` pays for the syntax: 10,000 keys parse in 1.34 s under
  `roc test` against 1.28 s before, a 10,000-element array in 0.26 s against
  0.20 s.
- **The tree (`TomlSyntax`), written back by concatenation:**
  - `File : { bom, lines }`; `bom` is `"\u(FEFF)"` or `""`.
  - `Line : { indent, body, trailing, ending }` with `body` one of `Blank`,
    `Pair(Pair)`, `Header(Header)`. `trailing` is spaces and a comment;
    `ending` is `"\n"`, `"\r\n"`, or `""` only on the last line. A key/value
    whose value spans lines (multi-line string, array, inline table) is one
    line. Spaces at the end of a file with no line break are a last `Blank`
    line. Sections are not nested: a header's section is its lines up to the
    next header (`TomlSyntax.section_end(lines, position)`).
  - `Header : { is_array, open, key, close }`, `open`/`close` the spaces inside
    the brackets.
  - `Key : { parts : List({ raw, name }), dots }`: `raw` the spelling with
    quotes, `dots` the `len - 1` separators with their spaces (`" . "`).
  - `Pair : { key, equals, node }`, `equals` the text from the key's end to the
    value (`" = "`).
  - `Node :=` `Scalar({ raw, value })` (the spelling and its `Value`),
    `Array({ elements, close })`, `InlineTable({ entries, close })`.
    `Element : { before, node, after, has_comma }` and
    `Entry : { before, pair, after, has_comma }`: `before` is the trivia
    after `[`, `{` or the previous comma; `after` the trivia up to the comma
    or bracket; `close` the trivia after the last comma (or all of it when
    empty) up to the bracket. Every element but the last has a comma; the last
    has one exactly when the source has a trailing comma, and then `close`
    holds what follows it.
  - `Node` is written out in full (no alias backing, per step 3) and has no
    `is_eq`, so `Line` and `File` do not support `==`; tests compare their
    text.
- **The lookup structure (`TomlIndex`)** is rebuilt from lines, not stored:
  `build(lines)` walks them through `TomlTree` exactly as parsing does (so
  paths, `Index`es of `[[x]]` elements and table origins agree with `parse`)
  and answers `{ value, pieces, containers }`. `to_value` is its `value`.
  - `Piece : { path, form, address, key_length }` for every header
    (`Header`, `ArrayHeader`), key/value line (`Pair`), inline-table entry
    (`InlineEntry`) and array element (`Element`), in document order. `path`
    is the full path defined (`[bin, Index(1), sub]` for `[bin.sub]` in the
    second element). `address : { line, route }`: the line, then element and
    entry positions down from that line's node (`t = { a = [1, { b = 2 }] }`
    puts `b` at route `[0, 1, 0]`). `key_length` is how many trailing path
    segments the piece's own key spells, so `path.drop_last(key_length)` is
    the table the piece sits in (a header's is its parent).
  - `Container : { path, kind }` for every table and array: `Root`,
    `Implicit` (only named on the way to a header), `Header` (defined by
    `[x]`, and `[[x]]` elements), `Dotted` (made by dotted keys outside
    inline tables, including one later extended by a deeper header),
    `ArrayOfTables`, `Inline` (an inline table or a dotted table inside one),
    `Array` (an array value).
  - For step 7b: D-S3-40 removal is `pieces_under(index, prefix)` (headers
    take their sections by `section_end`, pairs their line, entries and
    elements their route); D-S3-46's "header-created" is `Implicit` or
    `Header`, dotted-created is `Dotted`; D-S3-41's parent rules read
    `container_at(parent)` and, for "all children inline", the pieces with
    `key_length` 1 directly under it; D-S3-53's family is the `Header` and
    `ArrayHeader` pieces whose path starts with the parent path, ending at the
    last one's `section_end`; D-S3-20's inserted line ending is the table's
    last `Pair` line's `ending`, else the first non-empty ending in the file.
    Rebuild the index after each edit rather than patching it.
  - The walk cannot fail on a parsed document; if an edit leaves lines that
    `TomlTree` refuses, `build` crashes with a message naming that, so 7b's
    expects and suite find an invalid edit at once instead of a silent
    `Table([])`.
- **`TomlDocument`:** `Document :: { bom, lines }` (opaque) with `to_str`,
  `to_value` and `get` as its methods (`doc.to_str()`), and module functions
  `parse`, `file`, `from_file` and `index` for the component. Measured: a
  `::` record's fields are readable by functions outside its method block in
  the same file, and methods reach users through `Toml.Document`'s alias.
  Step 7b adds `set`, `set_with`, `remove`, `append` to `Document`'s block,
  delegating to its own module(s) over `TomlSyntax.File` and `TomlIndex`
  through `file`/`from_file`. `EditErr` and `Edit` are defined there and
  aliased in `Toml` as `Err` and `EncodeErr` are.
- **`get` choices where the log is silent:** it looks up in `to_value` (linear,
  per D-S3-8). `NotFound` carries the path through the missing key or the
  index past the end (`[bin, Index(2)]` for `[bin, Index(2), name]`);
  `NotATable` and `NotAnArray` carry the path of the value the key or index
  was applied to (`[server, port]` for `[server, port, x]` where `port` is an
  integer; `[]` for an index into the root).
- **Measured compiler behaviour:**
  - A fold whose state nests a record holding lists copies those lists on
    every append (`{ values, found: { pieces, … } }`: 10,000 appends 540 ms,
    also when destructured in the lambda's pattern); a flat state record or
    plain-parameter recursion does not (10 ms). `TomlIndex`'s folds keep flat
    states; this took an array of 10,000 elements' `to_value` from 1.5 s to
    0.3 s. Walking 10,000 keys costs what parsing them does (the `TomlTree`
    work).
  - `Toml.Edit`'s optional fields read as `edit.?table ?? Auto`; a record
    pattern with a default (`|{ table ? Auto }|`) does not parse.
  - Record patterns in `match` must name every field
    (`Ok({ body: Pair(pair) })` against a `Line` is a type mismatch).
  - `crash` accepts a `Str` constant, not only a literal.
