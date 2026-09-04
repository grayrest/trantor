//! Resolution: parse the wiring DAG, compute the mangled hosted-symbol map,
//! order the archives topologically (driver last), and run the compose-time
//! checks the linker will NOT do (H0c): exactly one runtime provider, and no
//! duplicate ownership of a hosted symbol.

use crate::manifest::{Driver, HostedLeaf, Interface, World};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One resolved hosted symbol: its mangled linker name and the Roc `Module.leaf`
/// it binds to in the composed `main.roc` hosted{} block.
#[derive(Debug, Clone)]
pub struct HostedBinding {
    pub symbol: String, // hematite__<head>__<stem>
    pub module: String, // e.g. "Fs"
    pub leaf: String,   // e.g. "file_read!"
}

#[derive(Debug)]
pub struct Resolved {
    /// Hosted bindings, in a deterministic order (by interface name).
    pub hosted: Vec<HostedBinding>,
    /// Host/driver component archive base names, topologically ordered
    /// (a component appears after the ones it imports from), driver LAST.
    pub archive_order: Vec<String>,
    /// Roc modules to import in the composed main.roc.
    pub imports: Vec<String>,
    /// The driver component name.
    pub driver: String,
    /// Interface name -> its shipped Roc module (for copying binding modules).
    pub interface_modules: BTreeMap<String, String>,
    /// Roc-implemented wiring points: (module, providing component). The module
    /// is copied from the shim component (it ships the binding module WITH
    /// bodies), and it gets NO hosted{} entry (D13: a Roc shim forwards).
    pub roc_impls: Vec<(String, String)>,
}

/// Sanitize a component name into the identifier segment of a mangled linker
/// symbol: roc requires hosted symbols to be valid C identifiers, so a name
/// like `std-stdio` becomes `std_stdio`. Host code must use the same form.
pub fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Parse a wiring expression like `audit(capstdfs)` or `stdio`. Returns the
/// chain head-first: `["audit", "capstdfs"]` or `["stdio"]`.
fn parse_chain(expr: &str) -> Vec<String> {
    let expr = expr.trim();
    match expr.split_once('(') {
        None => vec![expr.to_string()],
        Some((head, rest)) => {
            let inner = rest.trim_end_matches(')');
            let mut v = vec![head.trim().to_string()];
            v.extend(parse_chain(inner));
            v
        }
    }
}

pub fn resolve(dir: &Path, world: &World, driver: &Driver) -> Result<Resolved, String> {
    let driver_name = world.world.driver.clone();

    // --- hosted symbol map, in interface-name order (deterministic) ---
    let mut hosted = Vec::new();
    let mut interface_modules = BTreeMap::new();
    let mut roc_impls = Vec::new();
    for (iface_name, chain_expr) in &world.wiring {
        let chain = parse_chain(chain_expr);
        let head = chain.first().ok_or_else(|| format!("empty wiring for {iface_name}"))?;
        let iface: Interface = crate::manifest::load_interface(dir, iface_name)?;
        interface_modules.insert(iface_name.clone(), iface.module.clone());
        let head_kind = world.components.get(head).map(|c| c.kind.as_str()).unwrap_or("host");
        if head_kind == "roc" {
            // D19: a Roc shim serves Roc consumers. It ships the binding module
            // WITH bodies (forwarding to its own impl), so no hosted symbol.
            roc_impls.push((iface.module.clone(), head.clone()));
        } else {
            for HostedLeaf { leaf, symbol_stem } in &iface.hosted {
                hosted.push(HostedBinding {
                    symbol: format!("hematite__{}__{symbol_stem}", sanitize(head)),
                    module: iface.module.clone(),
                    leaf: leaf.clone(),
                });
            }
        }
    }
    // io and any hosted-less interface still ships a module (e.g. IOErr); record it.
    for (name, _) in &world.interfaces {
        if !interface_modules.contains_key(name) {
            if let Ok(iface) = crate::manifest::load_interface(dir, name) {
                interface_modules.insert(name.clone(), iface.module);
            }
        }
    }

    // --- D18-C / H1b: every type the driver's requires names must be reachable.
    //     A missing one SEGFAULTS `roc build` (fault 0x138c) with no diagnostic,
    //     so reject it here, before roc ever runs. requires_uses must be a subset
    //     of the imports we will emit (exposes ∪ binding modules ∪ imports_extra).
    //     An app-facing type the driver's requires names must be in `exposes`
    //     (the app imports it as `pf.<Module>`); one that is imported but not
    //     exposed is the exact H1b segfault.
    let exposed: BTreeSet<&str> = world.world.exports.iter().map(|s| s.as_str()).collect();
    for used in &driver.requires_uses {
        if !exposed.contains(used.as_str()) {
            return Err(format!(
                "driver `requires` names `{used}` (requires_uses) but it is not in the world's \
                 exposes {:?}; roc build would SEGFAULT on this (H1b, fault 0x138c). \
                 Add `{used}` to [world].exports.",
                world.world.exports
            ));
        }
    }

    // --- checks the linker will not do (H0c) ---
    let runtime_providers: Vec<&String> = world
        .components
        .iter()
        .filter(|(_, c)| c.provides_runtime)
        .map(|(n, _)| n)
        .collect();
    if runtime_providers.len() != 1 {
        return Err(format!(
            "exactly one component must set provides_runtime = true (found {}: {:?}); \
             the linker would silently first-wins duplicate roc_alloc (H0c)",
            runtime_providers.len(),
            runtime_providers
        ));
    }
    let mut owners: BTreeSet<&str> = BTreeSet::new();
    for b in &hosted {
        if !owners.insert(&b.symbol) {
            return Err(format!("hosted symbol {} owned by more than one component", b.symbol));
        }
    }

    // --- archive order: host/driver components, topological, driver last ---
    let host_like: Vec<String> = world
        .components
        .iter()
        .filter(|(n, c)| c.kind != "roc" && **n != driver_name)
        .map(|(n, _)| n.clone())
        .collect();
    // edges. Two sources:
    //  (1) each link of a wiring chain depends on the NEXT link (an interposer
    //      like audit -> capstdfs): link[i] depends on link[i+1].
    //  (2) a component importing interface I that is NOT itself in I's chain
    //      depends on the chain HEAD.
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for name in &host_like {
        deps.insert(name.clone(), BTreeSet::new());
    }
    for expr in world.wiring.values() {
        let chain = parse_chain(expr);
        for pair in chain.windows(2) {
            if let Some(d) = deps.get_mut(&pair[0]) {
                d.insert(pair[1].clone());
            }
        }
    }
    for name in &host_like {
        let c = &world.components[name];
        for imp in &c.imports {
            if let Some(expr) = world.wiring.get(imp) {
                let chain = parse_chain(expr);
                if !chain.contains(name) {
                    if let Some(head) = chain.into_iter().next() {
                        deps.get_mut(name).unwrap().insert(head);
                    }
                }
            }
        }
    }
    let mut ordered = Vec::new();
    let mut placed: BTreeSet<String> = BTreeSet::new();
    // Kahn's with alphabetical tiebreak for determinism.
    while ordered.len() < host_like.len() {
        let mut progressed = false;
        for name in &host_like {
            if placed.contains(name) {
                continue;
            }
            if deps[name].iter().all(|d| placed.contains(d) || !host_like.contains(d)) {
                ordered.push(name.clone());
                placed.insert(name.clone());
                progressed = true;
            }
        }
        if !progressed {
            return Err(format!("cycle in host component imports among {host_like:?}"));
        }
    }
    ordered.push(driver_name.clone()); // driver archive last

    // --- imports for main.roc: hosted binding modules (wiring order),
    //     pure-Roc exported modules, then shared type modules (io etc.) ---
    let mut imports = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for b in &hosted {
        if seen.insert(b.module.clone()) {
            imports.push(b.module.clone());
        }
    }
    // pure-Roc exported modules (e.g. Path) — from world exports not already imported
    for m in &world.world.exports {
        if seen.insert(m.clone()) {
            imports.push(m.clone());
        }
    }
    // driver's extra imports (e.g. IOErr)
    for m in &driver.imports_extra {
        if seen.insert(m.clone()) {
            imports.push(m.clone());
        }
    }

    Ok(Resolved {
        hosted,
        archive_order: ordered,
        imports,
        driver: driver_name,
        interface_modules,
        roc_impls,
    })
}
