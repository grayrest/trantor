//! Component `cell`: host-backed mutable state (D12). Fixture-minimal: one
//! process-global Str slot behind a Mutex. Proves a Roc shim can hold state
//! across calls through the host. Both boundaries extern "C-unwind" (H0d).
use hematite_abi as abi;
use abi::RocStr;
use std::sync::Mutex;

static SLOT: Mutex<String> = Mutex::new(String::new());

/// hosted `Cell.put! : Str => {}`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__cell__put(value: RocStr) {
    *SLOT.lock().unwrap() = value.as_str().to_string();
}

/// hosted `Cell.get! : {} => Str`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__cell__get() -> RocStr {
    RocStr::from_str(&SLOT.lock().unwrap(), abi::host())
}
