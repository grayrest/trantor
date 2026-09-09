//! Manifest parsing: `world.toml`, per-interface `interface.toml`, and the
//! driver's `driver.toml`. These are the two-format split of D18 — the TOML
//! side (composition), with the `.wit`-shaped interface side deferred to the
//! Roc binding modules the interfaces ship.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct World {
    pub world: WorldMeta,
    #[serde(default)]
    pub interfaces: BTreeMap<String, InterfaceRef>,
    pub components: BTreeMap<String, Component>,
    pub wiring: BTreeMap<String, String>,
    /// External Roc packages the composed platform declares (`packages { alias: "url" }`),
    /// so verbatim modules that `import alias.Module` keep working (D18).
    #[serde(default)]
    pub packages: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct WorldMeta {
    pub name: String,
    pub driver: String,
    #[serde(default)]
    pub exports: Vec<String>,
    /// Symbols the world explicitly declares as a shared/deduplicated native
    /// dependency, exempting them from the H0c archive collision scan (the
    /// escape hatch the policy names). Each entry is a source symbol name
    /// (no leading `_`), either exact (`sqlite3_open`) or a trailing-`*` prefix
    /// glob (`sqlite3_*`) to cover a whole vendored library. Empty by default;
    /// no baseline world needs it (sole-vendor), but a world that intends to
    /// share one native across two components declares that intent here rather
    /// than letting the linker first-wins silently.
    #[serde(default)]
    pub shared_symbols: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InterfaceRef {
    /// The versioned interface identifier (e.g. `roc:io/error@0.1.0`). Part of
    /// the manifest schema and carried for a future registry resolve; the
    /// composer currently keys on the interface's map name, not this.
    #[allow(dead_code)]
    pub source: String,
}

#[derive(Debug, Deserialize)]
pub struct Component {
    pub kind: String, // "host" | "roc" | "driver"
    /// Component implementation language (always `rust` today). Accepted for
    /// forward-compat (D-note: interfaces may be authored in Zig/WIT later);
    /// the composer does not branch on it yet.
    #[serde(default)]
    #[allow(dead_code)]
    pub lang: Option<String>,
    #[serde(default)]
    pub imports: Vec<String>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub provides_runtime: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub requires_uses: Vec<String>,
    /// Per-component Cargo feature knob (HC0). `features` is the exact set of
    /// the host crate's own Cargo features to make default-on in this
    /// composition; the composer writes it as the crate's `[features] default`.
    /// `default_features = false` is the "turn the authored default off"
    /// spelling (with an empty `features`, `default = []`). Both omitted: the
    /// component's authored `Cargo.toml` is left untouched. Host components
    /// only; the general knob the `tls` feature rides (H12).
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub default_features: Option<bool>,
    /// Test-only scaffolding (e.g. an HTTP test server). The app still links it
    /// (roc needs its symbols), but `publish` omits its archive from the
    /// baseline so test peers never ship. Default false.
    #[serde(default)]
    pub test_only: bool,
    /// macOS system frameworks this component links (e.g. `CoreFoundation`,
    /// pulled in by turso via chrono/iana_time_zone). roc links a framework only
    /// from a platform-bundled sysroot, so `hematite build` generates
    /// `platform/targets/macos-sysroot` containing exactly the frameworks the
    /// world's components declare — and none, skipping the sysroot entirely,
    /// when no component declares any. Mirrors the crate's own
    /// `#[link(name = "…", kind = "framework")]`. Host components only.
    #[serde(default)]
    pub frameworks: Vec<String>,
}

/// One interface's `interface.toml`: the Roc module it ships and the hosted
/// leaves it declares.
#[derive(Debug, Deserialize)]
pub struct Interface {
    pub module: String,
    #[serde(default)]
    pub hosted: Vec<HostedLeaf>,
    /// Resource types this interface declares (P5). A resource is a refcounted
    /// opaque host handle spelled `Name :: Box(U64)` in the shipped binding
    /// module; the host builds it with `hematite_abi::resource::new` and the
    /// driver's `roc_dealloc` runs its destructor on the last Roc drop.
    #[serde(default)]
    #[allow(dead_code)]
    pub resources: Vec<ResourceDecl>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ResourceDecl {
    #[allow(dead_code)]
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HostedLeaf {
    pub leaf: String,        // e.g. "file_read!"
    pub symbol_stem: String, // e.g. "file_read" -> hematite__<component>__file_read
}

/// The driver's `driver.toml`: its spliced `requires`, provided adapter, and
/// declared cross-component type uses (D18-C).
#[derive(Debug, Deserialize)]
pub struct Driver {
    /// Single provided entrypoint (CLI-style). Either this pair or `provides`
    /// (the multi-entry list, reactor-style) must be given.
    #[serde(default)]
    pub provides_symbol: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub provided_fn: Option<String>,
    /// Multiple provided entrypoints (reactor-style, e.g. roc_im_init + roc_im_view).
    #[serde(default)]
    pub provides: Vec<Provided>,
    #[serde(default)]
    #[allow(dead_code)]
    pub requires_uses: Vec<String>,
    #[serde(default)]
    pub imports_extra: Vec<String>,
    pub requires: String,
    pub provided: String,
    /// A reactor driver (multi-provides, host-calls-app) ships its own host
    /// src/lib.rs; hematite generates only its Cargo.toml, not the CLI driver
    /// body. A CLI driver leaves this false and hematite generates the body.
    #[serde(default)]
    pub authored_host: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Provided {
    pub symbol: String,
    #[serde(rename = "fn")]
    pub func: String,
}

impl Driver {
    /// Normalized list of (linker symbol, roc fn) provided entrypoints.
    pub fn provided_entries(&self) -> Vec<(String, String)> {
        if !self.provides.is_empty() {
            self.provides.iter().map(|p| (p.symbol.clone(), p.func.clone())).collect()
        } else {
            match (&self.provides_symbol, &self.provided_fn) {
                (Some(s), Some(f)) => vec![(s.clone(), f.clone())],
                _ => vec![],
            }
        }
    }
}

pub fn load_world(dir: &Path, file: &str) -> Result<World, String> {
    let p = dir.join(file);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}

pub fn load_interface(dir: &Path, name: &str) -> Result<Interface, String> {
    let p = dir.join("interfaces").join(name).join("interface.toml");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}

pub fn load_driver(dir: &Path, component: &str) -> Result<Driver, String> {
    let p = dir.join("components").join(component).join("driver.toml");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}
