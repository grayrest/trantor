//! Reading an archive's global DEFINED symbols, in the object formats the scan
//! meets: macho (a macOS host, `nm -m`), ELF (a Linux host, `readelf -sW`) and
//! wasm32 (`llvm-readobj --symbols`). The policy of what to do with them lives
//! in `scan.rs`; this module only answers "which symbols does this archive
//! define at global scope?", with the same classification in every format — a
//! symbol counts only when it is a real definition that is neither
//! hidden/private nor weak.
//!
//! Measured: on macho the class is the attribute right after the section —
//! `external`, not `private external` / `weak external`; on ELF it is the
//! symbol table's `GLOBAL` binding with `DEFAULT`/`PROTECTED` visibility (Rust's
//! compiler-builtins and std internals are `HIDDEN` or `WEAK` there); on wasm it is the
//! absence of the `BINDING_WEAK` / `BINDING_LOCAL` / `VISIBILITY_HIDDEN` /
//! `UNDEFINED` flags (`llvm-nm` alone prints hidden symbols such as
//! compiler-builtins' `__popcountsi2` and LLVM's `anon.*.llvm.*` constants as
//! ordinary `T`/`D` and would report 100+ false collisions).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where Homebrew installs the LLVM binutils, which Xcode does not ship.
const HOMEBREW_LLVM_BIN: &str = "/opt/homebrew/opt/llvm/bin";

/// An LLVM tool from `$LLVM_BIN`, else Homebrew's llvm when it is installed,
/// else the bare name for `PATH` to find. Linux distributions put the LLVM
/// binutils on `PATH` (`apt install llvm`), so a Homebrew-only default made
/// every lookup there a spawn failure on a directory that cannot exist.
pub fn llvm_tool(name: &str) -> PathBuf {
    if let Ok(bin) = std::env::var("LLVM_BIN") {
        return Path::new(&bin).join(name);
    }
    let homebrew = Path::new(HOMEBREW_LLVM_BIN).join(name);
    if homebrew.exists() {
        homebrew
    } else {
        PathBuf::from(name)
    }
}

/// How an archive's symbols are read: macho `nm -m`, ELF `readelf -sW`, or
/// `llvm-readobj` (wasm32 members). ELF and wasm names carry no macho
/// underscore.
#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Macho,
    Elf,
    Wasm,
}

impl Format {
    /// The format of the archives cargo builds on this host: the native path
    /// never passes cargo a `--target`, so the host decides.
    pub fn native() -> Format {
        if cfg!(target_os = "macos") {
            Format::Macho
        } else {
            Format::Elf
        }
    }
}

/// Is this a Rust-mangled symbol? v0 is `_R…` (macho `__R…`); the legacy
/// Itanium form is `_ZN…` (macho `__Z…`). wasm names carry no macho underscore.
pub fn is_rust_mangled(nm_name: &str, format: Format) -> bool {
    match format {
        Format::Macho => nm_name.starts_with("__R") || nm_name.starts_with("__Z"),
        Format::Elf | Format::Wasm => nm_name.starts_with("_R") || nm_name.starts_with("_ZN"),
    }
}

/// Parse `nm -m` output, returning the set of PLAIN-external DEFINED symbol names
/// (leading `_` kept). `nm -m` prints one symbol per line as
/// `<addr> (<section>) <attrs> [annotations…] <name>`; a definition has a real
/// section (not `undefined`) and the attribute token immediately after `)` is
/// exactly `external` (not `private external`, `weak external`,
/// `weak private external`, or `non-external`). Between the attrs and the name
/// nm may insert bracketed annotations (`[cold func]`, `[referenced dynamically]`,
/// …), so the name is taken as the last whitespace token — never the text right
/// after `external `.
fn plain_external_defs(nm_output: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in nm_output.lines() {
        // Split once on ") " into `<addr> (<section>` and `<attrs> … <name>`.
        let Some((head, rest)) = line.split_once(") ") else {
            continue;
        };
        // Section is what follows the last '(' in head.
        let Some(section) = head.rsplit('(').next() else {
            continue;
        };
        if section == "undefined" {
            continue; // a reference, not a definition
        }
        // A plain-external definition's attribute string starts with "external ";
        // "weak external", "private external", "weak private external" and
        // "non-external" all begin with a different word.
        if !rest.starts_with("external ") {
            continue;
        }
        // The symbol name is the final token (symbols carry no spaces); this
        // skips any `[cold func]`-style annotation nm places before it.
        if let Some(name) = rest.split_whitespace().next_back() {
            out.insert(name.to_string());
        }
    }
    out
}

/// Parse `readelf -sW` output over an ELF archive, returning the plain-global
/// DEFINED symbols. Each symbol row is
/// `<num>: <value> <size> <type> <bind> <vis> [annotations…] <ndx> <name>`;
/// readelf may print a bracketed annotation after the visibility (aarch64's
/// `[VARIANT_PCS]`), so the section index and name are taken from the END of the
/// row rather than by column. Header, `File:` and blank lines have no `<num>:`
/// and are skipped; so is the unnamed null symbol.
fn elf_global_defs(readelf_output: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in readelf_output.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [num, _value, _size, ty, bind, vis, .., ndx, name] = f.as_slice() else {
            continue;
        };
        let is_symbol_row = num.strip_suffix(':').is_some_and(|n| n.parse::<u64>().is_ok());
        // A reference (UND) is not a definition; a section or file symbol is
        // not something two archives can collide on.
        let is_definition = *ndx != "UND" && !matches!(*ty, "SECTION" | "FILE");
        if is_symbol_row && is_definition && *bind == "GLOBAL" && matches!(*vis, "DEFAULT" | "PROTECTED") {
            out.insert(name.to_string());
        }
    }
    out
}

/// wasm symbol flags (`llvm-readobj --symbols`), from the wasm object-file
/// linking metadata: a definition can first-wins-collide only when none of
/// these is set — weak is coalesced, local and hidden never reach global scope.
const WASM_SYMBOL_BINDING_WEAK: u32 = 0x1;
const WASM_SYMBOL_BINDING_LOCAL: u32 = 0x2;
const WASM_SYMBOL_VISIBILITY_HIDDEN: u32 = 0x4;
const WASM_SYMBOL_UNDEFINED: u32 = 0x10;
const WASM_NON_GLOBAL_FLAGS: u32 =
    WASM_SYMBOL_BINDING_WEAK | WASM_SYMBOL_BINDING_LOCAL | WASM_SYMBOL_VISIBILITY_HIDDEN | WASM_SYMBOL_UNDEFINED;

/// Parse `llvm-readobj --symbols` output over a wasm archive, returning the
/// plain-global DEFINED function/data symbols — the counterpart of macho's
/// `external`-not-`private`/`weak` classification.
fn wasm_global_defs(readobj_output: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let (mut name, mut is_code_or_data, mut flags) = (None::<String>, false, 0u32);
    for line in readobj_output.lines() {
        let t = line.trim();
        if t == "Symbol {" {
            (name, is_code_or_data, flags) = (None, false, 0);
        } else if let Some(n) = t.strip_prefix("Name: ") {
            name = Some(n.to_string());
        } else if let Some(ty) = t.strip_prefix("Type: ") {
            is_code_or_data = ty.starts_with("FUNCTION") || ty.starts_with("DATA");
        } else if let Some(rest) = t.strip_prefix("Flags [ (0x") {
            flags = u32::from_str_radix(rest.trim_end_matches(')'), 16).unwrap_or(u32::MAX);
        } else if t == "}" {
            if let Some(n) = name.take() {
                if is_code_or_data && flags & WASM_NON_GLOBAL_FLAGS == 0 {
                    out.insert(n);
                }
            }
        }
    }
    out
}

/// The global defined symbols of one archive. Xcode's `nm` exits nonzero on
/// rust-LLVM archives it can only partially read yet still prints the symbols
/// it can, so the exit status is ignored and stdout is used regardless
/// (matching the fixtures' `{ nm … || true; }` idiom).
pub fn archive_defs(archive: &Path, format: Format) -> Result<BTreeSet<String>, String> {
    let out = match format {
        Format::Macho => Command::new("nm").arg("-m").arg(archive).output(),
        Format::Elf => Command::new("readelf").arg("-sW").arg(archive).output(),
        Format::Wasm => Command::new(llvm_tool("llvm-readobj")).arg("--symbols").arg(archive).output(),
    }
    .map_err(|e| format!("run nm on {}: {e}", archive.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let defs = match format {
        Format::Macho => plain_external_defs(&text),
        Format::Elf => elf_global_defs(&text),
        Format::Wasm => wasm_global_defs(&text),
    };
    if defs.is_empty() {
        return Err(format!(
            "nm read no defined symbols from {} (build the world first?)",
            archive.display()
        ));
    }
    Ok(defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_external_defs_only() {
        let sample = "\
                     (undefined) external _sqlite3_step\n\
0000000000009004 (__TEXT,__text) external _sqlite3_open\n\
0000000000000000 (__TEXT,__text) private external __aarch64_cas8_acq\n\
---------------- (LTO,CODE) weak private external ___multi3\n\
0000000000000600 (__TEXT,__text) external __RNvMs_NtCsuFXAkltCeT_12trantor_abi9RocStr8from_str\n\
0000000000000800 (__TEXT,__text_cold) external [cold func] __RINvNtCsX_4core9panicking13assert_failed_rustls\n\
0000000000000010 (__TEXT,__text) non-external _local_helper\n";
        let defs = plain_external_defs(sample);
        assert!(defs.contains("_sqlite3_open")); // plain external, defined
        assert!(defs.contains("__RNvMs_NtCsuFXAkltCeT_12trantor_abi9RocStr8from_str"));
        // annotation before the name: the mangled name, not "[cold func]", is captured
        assert!(defs.contains("__RINvNtCsX_4core9panicking13assert_failed_rustls"));
        assert!(!defs.contains("_sqlite3_step")); // undefined reference
        assert!(!defs.contains("__aarch64_cas8_acq")); // private external
        assert!(!defs.contains("___multi3")); // weak private external
        assert!(!defs.contains("_local_helper")); // non-external
    }

    #[test]
    fn rust_mangled_recognized() {
        assert!(is_rust_mangled("__RNvMs_NtCs_12trantor_abi9RocStr8from_str", Format::Macho));
        assert!(is_rust_mangled("__ZN4core3fmt3num", Format::Macho));
        assert!(!is_rust_mangled("_sqlite3_open", Format::Macho));
        assert!(!is_rust_mangled("_vendored_answer", Format::Macho));
        assert!(is_rust_mangled("_RNvMs_NtCs_12trantor_abi9RocStr8from_str", Format::Wasm));
        assert!(!is_rust_mangled("trantor__b__seed", Format::Wasm));
        assert!(is_rust_mangled("_RNvCs_12trantor_abi4host", Format::Elf));
        assert!(!is_rust_mangled("sqlite3_open", Format::Elf));
    }

    #[test]
    fn parses_elf_global_defs_by_binding_and_visibility() {
        let sample = "\
File: libprobe_a.a(probe_a.o)\n\
\n\
Symbol table '.symtab' contains 7 entries:\n\
   Num:    Value          Size Type    Bind   Vis      Ndx Name\n\
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND \n\
     1: 0000000000000000     0 SECTION LOCAL  DEFAULT    3 .text\n\
     2: 0000000000000000    24 FUNC    GLOBAL DEFAULT    3 trantor_probe_collision\n\
     3: 0000000000000000     0 NOTYPE  GLOBAL DEFAULT  UND roc_alloc\n\
     4: 0000000000000010    12 FUNC    GLOBAL HIDDEN     3 __udivti3\n\
     5: 0000000000000020     8 FUNC    WEAK   DEFAULT    3 weak_thing\n\
     6: 0000000000000030     8 FUNC    GLOBAL DEFAULT [VARIANT_PCS]     3 annotated_def\n\
     7: 0000000000000040     8 OBJECT  GLOBAL PROTECTED  4 DATA_SYM\n";
        let defs = elf_global_defs(sample);
        assert_eq!(
            defs.into_iter().collect::<Vec<_>>(),
            ["DATA_SYM", "annotated_def", "trantor_probe_collision"],
            "undefined, section, hidden and weak symbols are not global definitions"
        );
    }

    #[test]
    fn parses_wasm_global_defs_by_flags() {
        let sample = "\
File: libb.a(b.o)\n\
Symbols [\n\
  Symbol {\n    Name: trantor__b__seed\n    Type: FUNCTION (0x0)\n    Flags [ (0x0)\n    ]\n    ElementIndex: 0x4\n  }\n\
  Symbol {\n    Name: __popcountsi2\n    Type: FUNCTION (0x0)\n    Flags [ (0x4)\n      VISIBILITY_HIDDEN (0x4)\n    ]\n  }\n\
  Symbol {\n    Name: anon.abc.0.llvm.1\n    Type: DATA (0x1)\n    Flags [ (0x4)\n      VISIBILITY_HIDDEN (0x4)\n    ]\n  }\n\
  Symbol {\n    Name: roc_alloc\n    Type: FUNCTION (0x0)\n    Flags [ (0x10)\n      UNDEFINED (0x10)\n    ]\n  }\n\
  Symbol {\n    Name: weak_thing\n    Type: FUNCTION (0x0)\n    Flags [ (0x1)\n      BINDING_WEAK (0x1)\n    ]\n  }\n\
  Symbol {\n    Name: __stack_pointer\n    Type: GLOBAL (0x2)\n    Flags [ (0x10)\n    ]\n  }\n\
  Symbol {\n    Name: DATA_SYM\n    Type: DATA (0x1)\n    Flags [ (0x0)\n    ]\n  }\n\
]\n";
        let defs = wasm_global_defs(sample);
        assert!(defs.contains("trantor__b__seed"));
        assert!(defs.contains("DATA_SYM"));
        assert!(!defs.contains("__popcountsi2")); // hidden
        assert!(!defs.contains("anon.abc.0.llvm.1")); // hidden
        assert!(!defs.contains("roc_alloc")); // undefined
        assert!(!defs.contains("weak_thing")); // weak
        assert!(!defs.contains("__stack_pointer")); // a wasm global, not code/data
    }
}
