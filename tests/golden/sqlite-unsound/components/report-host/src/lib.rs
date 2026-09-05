//! Test-only host: print a line (this fixture has no stdio host).
use hematite_abi as abi;
use abi::RocStr;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__report_host__print(s: RocStr) {
    println!("{}", s.as_str());
    unsafe { s.decref(abi::host()); }
}
