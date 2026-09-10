//! Manifest parsing: `world.toml`, per-interface `interface.toml`, and the
//! driver's `driver.toml`. These are the two-format split of D18 — the TOML
//! side (composition), with the `.wit`-shaped interface side deferred to the
//! Roc binding modules the interfaces ship.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
    /// A HOST cargo workspace that already owns this world's `path` components
    /// (relative to the world dir; D-H7-14). When set, hematite emits no
    /// workspace `Cargo.toml` and builds each component inside that workspace
    /// with this world's abi patched in — see `cargo.rs`.
    #[serde(default)]
    pub cargo_root: Option<String>,
    /// Where the interfaces live (relative to the world dir; default
    /// `interfaces`). Several worlds of one repo share one directory.
    #[serde(default)]
    pub interfaces_dir: Option<String>,
    /// wasm32 only (D-H7-31): build every component the SIZE-CORRECT way —
    /// `-Z build-std` with immediate-abort panics, opt-level z, fat LTO, one
    /// codegen unit, stripped — roc-solid's `dom-host` recipe, measured at a
    /// 12x smaller module than a plain staticlib (its D25). Needs the
    /// `rust-src` component; `RUSTC_BOOTSTRAP=1` is a stated supply-chain
    /// fact, not an incidental.
    #[serde(default)]
    pub wasm_size_correct: bool,
}

/// The directory holding `<interface>/interface.toml` for a world.
pub fn interfaces_dir(world_dir: &Path, world: &World) -> PathBuf {
    world_dir.join(world.world.interfaces_dir.as_deref().unwrap_or("interfaces"))
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
    /// Where the component lives, relative to the world dir, when it is not
    /// under `components/<name>/` (D-H7-4: a crate has one home —
    /// `path = "../../crates/svc-notes"`). Its Roc modules are looked up in
    /// `<dir>/roc/` first, then `<dir>/` (see `module_path`).
    #[serde(default)]
    pub path: Option<String>,
}

/// The directory a component's sources live in: `path` if declared, else
/// `components/<name>/` under the world dir.
pub fn component_dir(world_dir: &Path, name: &str, c: &Component) -> PathBuf {
    match &c.path {
        Some(p) => world_dir.join(p),
        None => world_dir.join("components").join(name),
    }
}

/// A component's Roc module file: `<dir>/roc/<Module>.roc` when the component
/// keeps its Roc beside its Rust (a driver crate shipping its contract
/// modules), else `<dir>/<Module>.roc` (the fixtures' flat layout).
pub fn module_path(component_dir: &Path, module: &str) -> PathBuf {
    let nested = component_dir.join("roc").join(format!("{module}.roc"));
    if nested.exists() {
        return nested;
    }
    component_dir.join(format!("{module}.roc"))
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
    /// `kind = "service"` (D-H7-5): the interface is a service whose command
    /// union is `module`'s nominal, spliced into the driver's `Cmd` as the
    /// wrapper variant `<Module>(<Module>)`; the wired component implements the
    /// D-H7-7 contract (`hematite__<c>__init/cmd/complete/gate`, `env`) instead
    /// of hosted leaves. Absent: an ordinary hosted interface.
    #[serde(default)]
    pub kind: Option<String>,
    /// A service's event union module (`NotesEvent`), spliced into the driver's
    /// `Event` as `<Module>(<EventModule>)`. One nominal per module (P0).
    #[serde(default)]
    pub event_module: Option<String>,
    /// A service's ambient block module (`AudioEnv`), spliced into the driver's
    /// `Env` record as `<snake(module)> : <EnvModule>`, read once per frame.
    #[serde(default)]
    pub env_module: Option<String>,
}

impl Interface {
    pub fn is_service(&self) -> bool {
        self.kind.as_deref() == Some("service")
    }
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
    #[serde(default)]
    pub requires: String,
    #[serde(default)]
    pub provided: String,
    /// Another driver's `driver.toml` whose CONTRACT this driver shares —
    /// `requires`, `provided`, `provides`, `requires_uses`, `imports_extra`
    /// (D-H7-30: roc-solid's two drivers speak one app contract, and a second
    /// copy of 300 lines of it would drift). Relative to this file. Only
    /// `authored_host` and `wasm_exports` stay this driver's own.
    #[serde(default)]
    pub contract_from: Option<String>,
    /// A reactor driver (multi-provides, host-calls-app) ships its own host
    /// src/lib.rs; hematite generates only its Cargo.toml, not the CLI driver
    /// body. A CLI driver leaves this false and hematite generates the body.
    #[serde(default)]
    pub authored_host: bool,
    /// Always emit the services shim, even in a world that wires no service
    /// (D-H7-33): a driver written against `abi::services` — init, dispatch,
    /// on_wake, gate — compiles in every world it serves, and a world with
    /// nothing wired gets a shim whose every call is a no-op.
    #[serde(default)]
    pub services_shim: bool,
    /// Functions the driver exports from a wasm32 module (roc's `exports:`
    /// target field, required on this compiler). Empty: the world emits no
    /// wasm32 target.
    #[serde(default)]
    pub wasm_exports: Vec<String>,
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

pub fn load_interface(dir: &Path, world: &World, name: &str) -> Result<Interface, String> {
    let p = interfaces_dir(dir, world).join(name).join("interface.toml");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}

pub fn load_driver(dir: &Path, world: &World) -> Result<Driver, String> {
    let name = &world.world.driver;
    let c = world
        .components
        .get(name)
        .ok_or_else(|| format!("[world].driver `{name}` is not a declared component"))?;
    let p = component_dir(dir, name, c).join("driver.toml");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    let mut d: Driver = toml::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))?;
    if let Some(rel) = &d.contract_from {
        let from = p.parent().unwrap_or(Path::new(".")).join(rel);
        let text = std::fs::read_to_string(&from).map_err(|e| format!("read {}: {e}", from.display()))?;
        let shared: Driver = toml::from_str(&text).map_err(|e| format!("parse {}: {e}", from.display()))?;
        d.requires = shared.requires;
        d.provided = shared.provided;
        d.provides = shared.provides;
        d.provides_symbol = shared.provides_symbol;
        d.provided_fn = shared.provided_fn;
        d.requires_uses = shared.requires_uses;
        d.imports_extra = shared.imports_extra;
    }
    if d.requires.is_empty() || d.provided.is_empty() {
        return Err(format!("{}: `requires` and `provided` are required (or `contract_from`)", p.display()));
    }
    Ok(d)
}
