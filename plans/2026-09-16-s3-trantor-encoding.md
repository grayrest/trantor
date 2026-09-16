# S3 — `trantor-encoding`

**Design log:** `notes/2026-09-16-s3-encoding-design-log.md`, D-S3-1 to D-S3-44.
Where a decision is amended, the later one governs; each amended decision names
its amendments at the top. Repos: `~/dev/roc/trantor-encoding` (new),
`~/dev/roc/trantor-temporal`, `~/dev/roc/trantor-hash`, `trantor` (docs).
Source for CSV: `~/dev/roc/playground/csv`, copied, never modified or deleted
(D-S3-18); its API is not preserved (D-S3-39). Gate at every commit:
`trantor test .` in each touched package passes with no warnings.

## Why

D-S1-11 step 3. Nothing in Roc reads or writes TOML or Base64; hex exists only
for builtin digests; the CSV package sits outside trantor. trantor's own
manifests are TOML, and a Roc tool that edits them needs a lossless document.

## Package

```
trantor-encoding/
  package.toml
  README.md
  components/
    base64/  Base64.roc Hex.roc
    csv/     Csv.roc CsvParse.roc CsvEmit.roc ... (from the playground)
    toml/    Toml.roc TomlLex.roc TomlParse.roc TomlFormat.roc TomlWrite.roc TomlDocument.roc ...
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

[components.csv]
kind = "roc"
exports = ["Csv", "CsvParse", "CsvEmit"]      # plus whatever internal modules result

[components.toml]
kind = "roc"
exports = ["Toml", "TomlLex", "TomlParse"]    # plus whatever internal modules result

[dev-deps]
trantor-cli = { path = "../trantor-cli" }
```

Internal modules are prefixed with their format and are component exports
only. File layout inside a component is the implementer's; the public surface
is not.

## Surface

Signatures by decision:

| Module | Decisions |
|---|---|
| Base64 | D-S3-2, D-S3-37.7, D-S3-44.11 |
| Hex | D-S3-3, D-S3-44.11 |
| Csv | D-S3-4, D-S3-5, D-S3-39, D-S3-36 (dates) |
| Toml values and errors | D-S3-8, D-S3-11, D-S3-24, D-S3-28, D-S3-30, D-S3-32, D-S3-38, D-S3-44.8 |
| Toml typed | D-S3-9, D-S3-33, D-S3-38 |
| Toml writing | D-S3-15, D-S3-16, D-S3-37.3–6, D-S3-43, D-S3-44.7, .9, .12 |
| Toml editing | D-S3-10, D-S3-29, D-S3-31, D-S3-40, D-S3-41, D-S3-42 |
| Dates | D-S3-23, D-S3-26, D-S3-44.3 |

Format types are nested in their module (measured: `Fmt :: [].{ Enc := … }`
with the entry point on `Fmt`). Entry points that need a format's state type
live in the module that defines it.

### The date contract (D-S3-22, D-S3-23, D-S3-26, D-S3-30, D-S3-36, D-S3-44.3)

Defined by TOML's and CSV's formats; the `encode_*` half also by trantor-hash's
`HashFormat`:

```roc
parse_local_date : fmt, state -> Try({ value : { year : I32, month : U8, day : U8 }, rest : state }, [Mismatch({ key : List([Key(Str), Index(U64)]), expected : Str }), ..])
encode_local_date : fmt, { year : I32, month : U8, day : U8 }, state -> Try(state, err)
parse_local_time / encode_local_time             # { hour : U8, minute : U8, second : U8, millisecond : U16, microsecond : U16, nanosecond : U16 }
parse_local_datetime / encode_local_datetime     # { date, time }
parse_offset_datetime / encode_offset_datetime   # { date, time, offset : { minutes : I16 } }
```

- The parse row holds `Mismatch` and no other tag. A hand-written
  `parser_for` names the row closed in its `where` clause and reopens it
  (`? |Mismatch(m)| Mismatch(m)`); an error variable there compiles at the top
  level and fails inside records.
- The date encode methods take the format first. The other `encode_*` methods
  do not (derived containers and leaf encoders with a format argument are arity
  errors); a format with dates mixes both forms.
- Encoding keeps an error variable.

### Measure-inside-containers rule

Every hand-written `parser_for`/`encoder_for` (TOML's four date types,
`Toml.Value`, temporal's `PlainDate`/`PlainTime`) is tested inside a record, a
list, a nested record and a `Dict` value, never only at the top level. Dates are
also tested through CSV, in records (CSV has no lists or dicts).

## Work — commit at each

1. **Scaffold, Base64, Hex** (D-S3-2, D-S3-3, D-S3-27, D-S3-37.7, D-S3-44.11).
   - Repo, `package.toml`, README stub.
   - Base64 standard and URL-safe. Decode scans left to right: the first
     out-of-alphabet byte or misplaced `=` (partial padding such as `QQ=`, at
     the `=`) is `InvalidBase64(byte index)`; nonzero trailing bits are
     refused; only a clean scan checks `InvalidLength` (length ≡ 1 mod 4).
   - Hex: lowercase out, either case in; `InvalidHex(byte index)` before
     `OddLength`.
   - Expects: RFC 4648 §10 vectors both ways; every rejection and precedence.
2. **CSV moved in** (D-S3-1, D-S3-4, D-S3-5, D-S3-20, D-S3-39).
   - Copy the playground modules into `components/csv`: one public `Csv` with
     `Csv.Table`, `Row`, `Dialect` (`csv`, `tsv`), `Err`; format types nested;
     `Tsv` gone; the Bool workaround removed.
   - TOML's error model: `Csv.Err` (`Syntax`, `RaggedRow`, `MissingHeader`;
     1-based lines, code-point columns) and `err_to_str`. `decode`/
     `decode_with` stop at the first error with `Parse`,
     `Mismatch([Index(row), Key(column)])` (rows from 0 after the header) or
     `MissingRequiredField(Str)`; `BadCells` and `Bad` are removed.
     `encode`, `encode_with`, `encode_columns`, `encode_columns_with` return
     `Try` with `Csv.EncodeErr` (no cases until step 5 adds dates).
   - Skip one leading U+FEFF on every parse path.
   - Port the 156 assertions and `Stress.roc`, rewritten for this model. If the
     cross-package miscompile from `e2b81982` appears, record a minimal
     reproduction here and ask.
   - Expects: `Bool` encoding; `encode_columns` errors; columns after astral
     characters and after CRLF; a row's first bad cell with its path.
3. **TOML reading** (D-S3-7, D-S3-8, D-S3-11, D-S3-20, D-S3-32, D-S3-34,
   D-S3-35, D-S3-37.5, D-S3-38, D-S3-43, D-S3-44.1–2, .4, .6, .8).
   - Lexer and parser for 1.1.0 into nominal `Toml.Value`. Its `is_eq`:
     entries sorted by key then compared; arrays in order; floats by `F64` with
     NaN equal. `Toml.Float` keeps the spelling, with `to_f64`, `to_dec`,
     `float_from_f64`, `float_from_dec` and `is_eq`.
   - ISO calendar validation, second 60 refused, offsets within ±23:59, `I64`
     integers, fraction digits past nanoseconds cut off, one depth of 128 over
     all nesting, duplicates found with a `Dict`, line breaks in multi-line
     strings read as `\n`, leading U+FEFF skipped, CRLF accepted.
   - `Toml.Err` (1-based lines, code-point columns, first error only, segment
     keys), `Toml.parse`, `Toml.err_to_str`, `Toml.Segment`, `Toml.path`.
   - Corpus: `toml-lang/toml-test` checked in under
     `tests/toml-conformance/corpus`, README with commit and case counts.
     `tests/toml-conformance` runs the 1.1.0 valid and invalid lists and the
     1.0.0 valid list through `parse`; a hand-written JSON reader (test code)
     loads expected output, converted to `Value` and compared by its equality;
     invalid files must be `Err`.
   - Expects:
     - each error kind and position;
     - depth 128 accepted and 129 refused via arrays, inline tables, headers
       and dotted keys;
     - `I64` bounds (`-9223372036854775808` ok, `9223372036854775808` and
       `0x8000000000000000` refused); offset overflow;
     - BOM; CRLF; LF and CRLF multi-line strings equal; an escaped `\r` kept;
       multi-line strings with `"""`, a trailing `"`, a leading newline;
     - `[[x]]` after a static array `x`; a duplicate inside the second
       `[[bin]]` carrying `Index`;
     - equality with duplicate keys and nested NaN; a reversed 10,000-key
       compare within a time budget.
4. **Typed TOML and writing** (D-S3-9, D-S3-15, D-S3-16, D-S3-17, D-S3-24,
   D-S3-28, D-S3-33, D-S3-37.2–4, .6, .8, D-S3-38, D-S3-43, D-S3-44.4, .6–7,
   .9, .12).
   - TOML format over `Value`: `parse_*` for every scalar width
     (range-checked; integers into floats when exact; floats into integers are
     `Mismatch`), `parse_dec` from `Toml.Float`'s spelling, records, lists,
     `Dict` with `Str` keys, the date contract. `Mismatch` carries segment
     paths; `MissingRequiredField` is the compiler's `Str`.
   - `Toml.decode` (`parse` then `decode_value`; its `where` clause repeats
     `Parse(Toml.Err)`) and `Toml.decode_value`. The four date types (with
     `is_eq`) and `Toml.Value` get hand-written codecs by the date contract's
     pattern; `Value`'s are tied to TOML's format.
   - Writer: D-S3-15 layout; `Dict` keys sorted by bytes; non-empty bare keys
     (`""` quoted); D-S3-37.4 escapes with `\r` always escaped; kept float
     spellings when valid for the mode, else D-S3-37.6; fractions trimmed to at
     most 9 digits and omitted when zero; `Toml.Write` (`V1_1` = `\e`/`\xHH`,
     seconds omitted when seconds and fraction are zero). `to_str`,
     `to_str_with`, `encode`, `encode_with`, `encode_value`, all
     `Toml.EncodeErr` with segment keys; depth checked for `Value` trees and
     typed values. No `encode_null`, no `encode_tag`, only `encode_key_str`.
   - A test-only strict 1.0 checker in `tests/toml-conformance`, tested
     against both 1.0.0 lists (valid accepted, invalid refused) and applied to
     every default-mode output.
   - Expects:
     - golden output for both modes: every escape class, `1.0`, `-0.0`,
       `1e300`, `1e-7`, `inf`, `-inf`, `nan`, kept spellings, fraction digits,
       an empty key, sorted `Dict`;
     - `decode(encode(v)) == v` over every supported type, `Dec` past 17 digits
       included, and a `Dec` through `encode_value`/`decode_value`;
     - the measure-inside-containers rule; every `EncodeErr`.
   - Conformance suite: `parse(to_str(v)) == v` for every valid file in both
     modes.
5. **Dates across packages** (D-S3-22, D-S3-25, D-S3-36, D-S3-37.1, .9,
   D-S3-39, D-S3-44.10).
   - trantor-encoding, CSV's eight date methods: read RFC 3339, a space for
     `T`, and missing seconds (TOML 1.1's extension); write TOML's default-mode
     spelling; parse fails with `Mismatch`; `Csv.EncodeErr` gains
     `InvalidDate`/`InvalidTime`/`InvalidOffset` (a year outside 0–9999
     included).
   - trantor-temporal:
     - `PlainDate`/`PlainTime` codecs over the contract, with no import of
       trantor-encoding; encode writes ISO fields and ignores `cal`;
     - `plain_date_from_fields`, `plain_time_from_fields`;
     - `zoned_from_offset!`, `to_offset_datetime!` (converting the instant to
       the rounded offset);
     - trantor-encoding in `[dev-deps]`, and a suite decoding and encoding
       `PlainDate`/`PlainTime` fields through TOML inside records, lists and
       dicts and through CSV in records; `Csv.encode` of a year-10000 date;
     - expects for a negative offset under an hour, an IANA zone losing its
       name, and a local-mean-time offset keeping the instant;
     - README: JSON is a compile error; zone name and calendar are dropped.
   - trantor-hash: `HashFormat`'s four encode methods; golden expects pinning
     the layout; existing golden hashes unchanged; a README line.
6. **TOML editing** (D-S3-10, D-S3-20, D-S3-29, D-S3-30, D-S3-31, D-S3-34,
   D-S3-40, D-S3-41, D-S3-42, D-S3-43, D-S3-44.5).
   - `Toml.Document`: a lossless tree with trivia (whitespace, comments, line
     endings, BOM) and source spellings. `parse_document`, `to_str`,
     `to_value`, `get`, `set`, `set_with` (`Toml.Edit`), `remove`, `append`,
     all `Toml.EditErr`.
   - Styles `Auto`/`Inline`/`Header`/`Dotted` by D-S3-41, with
     `StyleNotPossible` and a `[header]` for the direct parent of an inline or
     dotted table; removal spans (D-S3-31) by path prefix (D-S3-40); `append`
     layouts; edits following existing layout, with `version` governing new
     constructs (D-S3-42); depth checks answering `TooDeep`.
   - Conformance suite: every valid file through `parse_document`/`to_str`
     byte-identical, and `to_value` equal to `parse`'s.
   - `tests/toml-edit`, one case each:
     - replace a value keeping its trailing comment; add a key to a header, a
       dotted and an inline table;
     - `Auto` producing inline in a `[deps]`-style table, header elsewhere,
       dotted under a dotted parent, header under a Cargo-style mixed parent,
       `[[x]]` for an array of tables; forced `Inline`/`Header`/`Dotted`;
       `StyleNotPossible`; `trantor add`'s first dependency under `Auto` and
       `Inline`; implicit parents (`components.x.kind` with no
       `[components]`);
     - remove a key with its comment block, a multi-line value, an
       inline-table entry, a header table with its sub-sections, a scattered
       dotted table, an implicit table, sub-sections before their parent, one
       `[[x]]` entry, the last key of each kind of table;
     - append to single-line, multi-line and `[[x]]` arrays, and inside a
       multi-line inline table under `V1_0`;
     - set a scalar over a `[section]` and the reverse; `Index` paths into
       `[[bin]]`; CRLF file; BOM file; `set_with` `V1_1`; a typed record
       through `encode_value` and `set`.
7. **Documentation.**
   - trantor-encoding README in trantor-temporal's style: setup, an example per
     module, each module's types and signatures, the TOML version policy,
     byte-stability (D-S3-17), `MissingRequiredField` naming only the field
     (D-S3-28), `Toml.Float` and exactness, the `Toml.Edit` annotation when
     stored, dotted and implicit tables vanishing with their last key, the
     Excel BOM note, second 60, known limits. `tests/readme` runs its
     examples.
   - trantor: D-S1-11 step 3 points at the package; implementation notes below.

## Implementation notes

*(Filled in as work lands: commits, measurements, deviations raised and their
answers.)*
