//! `[world] exports`, the app's `exposes`, defaulted from the driver's
//! `exposes` and extended by `[world] exports_extra` (D-H7-46).
//!
//! Every world that wired one driver used to restate that driver's app-facing
//! module list and append its own service modules (roc-solid: ten files, 53
//! names each). Adding a module to the driver meant editing every world, and
//! the lists drifted: one world lost a module nobody meant to drop. The driver
//! now names its app-facing modules once, and a world lists only what it adds.

use crate::manifest::{Driver, WorldMeta};
use std::path::Path;

/// Reject a driver whose `exposes` names a module it does not ship.
///
/// `exposes` is an allowlist over `exports`: a module in `exports` and not in
/// `exposes` still ships (an exposed module imports it) but no app can import
/// it. A name outside `exports` would reach the composed `exposes` with no
/// `.roc` file behind it, and `roc build` SEGFAULTS on that rather than
/// reporting it (the H1b class), so it is refused here, naming the module.
pub fn check_exposes(driver: &Driver, path: &Path) -> Result<(), String> {
    match driver.exposes.iter().find(|m| !driver.exports.contains(m)) {
        None => Ok(()),
        Some(m) => Err(format!(
            "{}: `exposes` names `{m}`, which is not in this driver's `exports`. \
             `exposes` is the part of `exports` apps may import; add `{m}` to \
             `exports` or drop it from `exposes`.",
            path.display()
        )),
    }
}

/// Fill `[world] exports` from the driver's `exposes` when the world omits it,
/// appending `[world] exports_extra` after it.
///
/// A world that states `exports` keeps it outright, as before this existed.
/// Stating both is refused rather than given a precedence: `exports` replaces
/// the driver's list and `exports_extra` extends it, so a world holding both
/// means one of them is stale, and either precedence would silently drop the
/// other list. An empty `exports` counts as omitted (TOML cannot tell the two
/// apart once parsed).
///
/// An extra that the driver already exposes, or that appears twice, is refused
/// as well: a world repeating a driver module is the drift this removes.
pub fn default_world_exports(world: &mut WorldMeta, driver_exposes: &[String]) -> Result<(), String> {
    if !world.exports.is_empty() {
        if world.exports_extra.is_empty() {
            return Ok(());
        }
        return Err(format!(
            "world `{}` states both `[world] exports` and `[world] exports_extra`. \
             `exports` is the whole list and replaces the driver's `exposes`; \
             `exports_extra` is appended to the driver's `exposes`. Keep one.",
            world.name
        ));
    }
    let mut list = driver_exposes.to_vec();
    for m in &world.exports_extra {
        if driver_exposes.contains(m) {
            return Err(format!(
                "world `{}` lists `{m}` in `exports_extra`, but its driver already exposes \
                 it. `exports_extra` holds only the world's own modules; drop `{m}`.",
                world.name
            ));
        }
        if list.contains(m) {
            return Err(format!("world `{}` lists `{m}` twice in `exports_extra`.", world.name));
        }
        list.push(m.clone());
    }
    world.exports = list;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(lists: &str) -> WorldMeta {
        toml::from_str(&format!("name = \"w\"\n{lists}")).unwrap()
    }

    fn driver(lists: &str) -> Driver {
        toml::from_str(lists).unwrap()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn should_default_world_exports_to_the_driver_exposes() {
        let mut w = world("");
        default_world_exports(&mut w, &names(&["Ui", "View"])).unwrap();
        assert_eq!(w.exports, ["Ui", "View"]);
    }

    #[test]
    fn should_append_exports_extra_after_the_driver_exposes() {
        let mut w = world("exports_extra = [\"Audio\", \"AudioEnv\"]\n");
        default_world_exports(&mut w, &names(&["Ui", "View"])).unwrap();
        assert_eq!(w.exports, ["Ui", "View", "Audio", "AudioEnv"]);
    }

    #[test]
    fn should_keep_a_stated_exports_list_unchanged() {
        let mut w = world("exports = [\"View\", \"Audio\"]\n");
        default_world_exports(&mut w, &names(&["Ui", "View"])).unwrap();
        assert_eq!(w.exports, ["View", "Audio"]);
    }

    #[test]
    fn should_reject_exports_and_exports_extra_together() {
        let mut w = world("exports = [\"Ui\"]\nexports_extra = [\"Audio\"]\n");
        let e = default_world_exports(&mut w, &[]).unwrap_err();
        assert!(e.contains("`[world] exports`") && e.contains("`[world] exports_extra`"), "{e}");
    }

    #[test]
    fn should_reject_an_extra_the_driver_already_exposes() {
        let mut w = world("exports_extra = [\"Ui\"]\n");
        let e = default_world_exports(&mut w, &names(&["Ui"])).unwrap_err();
        assert!(e.contains("`Ui`") && e.contains("already exposes"), "{e}");
    }

    #[test]
    fn should_reject_an_extra_listed_twice() {
        let mut w = world("exports_extra = [\"Audio\", \"Audio\"]\n");
        let e = default_world_exports(&mut w, &[]).unwrap_err();
        assert!(e.contains("`Audio` twice"), "{e}");
    }

    #[test]
    fn should_accept_exposes_within_exports() {
        let d = driver("exports = [\"Ui\", \"Hash\"]\nexposes = [\"Ui\"]\n");
        assert!(check_exposes(&d, Path::new("driver.toml")).is_ok());
    }

    #[test]
    fn should_reject_an_exposes_entry_missing_from_exports() {
        let d = driver("exports = [\"Ui\"]\nexposes = [\"Ui\", \"Ghost\"]\n");
        let e = check_exposes(&d, Path::new("driver.toml")).unwrap_err();
        assert!(e.contains("`Ghost`") && e.contains("not in this driver's `exports`"), "{e}");
    }
}
