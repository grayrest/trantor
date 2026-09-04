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
}

#[derive(Debug, Deserialize)]
pub struct WorldMeta {
    pub name: String,
    pub driver: String,
    #[serde(default)]
    pub exports: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InterfaceRef {
    pub source: String,
}

#[derive(Debug, Deserialize)]
pub struct Component {
    pub kind: String, // "host" | "roc" | "driver"
    #[serde(default)]
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
}

/// One interface's `interface.toml`: the Roc module it ships and the hosted
/// leaves it declares.
#[derive(Debug, Deserialize)]
pub struct Interface {
    pub module: String,
    #[serde(default)]
    pub hosted: Vec<HostedLeaf>,
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
    pub provides_symbol: String,
    #[allow(dead_code)]
    pub provided_fn: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub requires_uses: Vec<String>,
    #[serde(default)]
    pub imports_extra: Vec<String>,
    pub requires: String,
    pub provided: String,
}

pub fn load_world(dir: &Path) -> Result<World, String> {
    let p = dir.join("world.toml");
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
