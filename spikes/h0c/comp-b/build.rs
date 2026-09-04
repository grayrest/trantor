fn main() {
    cc::Build::new().file("csrc/vendor.c").compile("vendor");
    println!("cargo:rerun-if-changed=csrc/vendor.c");
}
