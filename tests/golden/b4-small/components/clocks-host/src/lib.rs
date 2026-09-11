//! roc:clocks host: wall-clock (nanos since epoch, U128), monotonic (nanos),
//! and the blocking sleep (P12). All owned-data returns.
use core::mem::ManuallyDrop;
use trantor_abi::*;
use std::sync::OnceLock;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__clocks_host__wall_now() -> ClocksWallNowResult {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => ClocksWallNowResult { payload: ClocksWallNowResultPayload { ok: ManuallyDrop::new(d.as_nanos()) }, tag: ClocksWallNowResultTag::Ok },
        Err(_) => ClocksWallNowResult { payload: ClocksWallNowResultPayload { err: [] }, tag: ClocksWallNowResultTag::Err },
    }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__clocks_host__monotonic_now() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__clocks_host__sleep_millis(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
