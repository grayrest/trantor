//! The baseline's one line of output. It cannot fail in a way the walkthrough
//! cares about, so the interface returns `{}` rather than a Try.
use trantor_abi as abi;
use abi::*;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__stdout_host__line(line: RocStr) {
    println!("{}", line.as_str());
    unsafe { line.decref(abi::host()); } // owned arg (B0): released after use
}
