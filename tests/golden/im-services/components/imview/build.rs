//! The fixture's proof of D-H7-27: trantor tells a driver's build what the
//! world wires (`TRANTOR_SERVICES`, sorted wiring keys) and which world it is
//! (`TRANTOR_WORLD`), so a driver serving several worlds can `cfg` the code
//! only one of them has. imview serves one world and merely checks the words
//! arrive; roc-solid's host-im turns them into `cfg(trantor_service = "…")`.
fn main() {
    println!("cargo:rerun-if-env-changed=TRANTOR_SERVICES");
    println!("cargo:rerun-if-env-changed=TRANTOR_WORLD");
    let services = std::env::var("TRANTOR_SERVICES").expect("trantor exports TRANTOR_SERVICES");
    assert_eq!(services, "bell,chime,echo,nudge,tick", "the wiring keys, sorted");
    assert!(std::env::var("TRANTOR_WORLD").is_ok(), "trantor exports TRANTOR_WORLD");
}
