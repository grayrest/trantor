//! The fixture's proof of D-H7-27: hematite tells a driver's build what the
//! world wires (`HEMATITE_SERVICES`, sorted wiring keys) and which world it is
//! (`HEMATITE_WORLD`), so a driver serving several worlds can `cfg` the code
//! only one of them has. imview serves one world and merely checks the words
//! arrive; roc-solid's host-im turns them into `cfg(hematite_service = "…")`.
fn main() {
    println!("cargo:rerun-if-env-changed=HEMATITE_SERVICES");
    println!("cargo:rerun-if-env-changed=HEMATITE_WORLD");
    let services = std::env::var("HEMATITE_SERVICES").expect("hematite exports HEMATITE_SERVICES");
    assert_eq!(services, "echo,tick", "the wiring keys, sorted");
    assert!(std::env::var("HEMATITE_WORLD").is_ok(), "hematite exports HEMATITE_WORLD");
}
