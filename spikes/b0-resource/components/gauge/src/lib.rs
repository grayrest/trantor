//! B0: a Counter resource with an open/close gauge. `open!` builds the resource
//! via `resource::new`; the Rust value's Drop is the destructor, which runs
//! when Roc drops the LAST reference (via the driver's roc_dealloc hook).
use trantor_abi as abi;
use abi::RocBox;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

pub static OPENS: AtomicU64 = AtomicU64::new(0);
pub static CLOSES: AtomicU64 = AtomicU64::new(0);

struct CounterState { id: u64, n: u64 }
impl Drop for CounterState {
    fn drop(&mut self) {
        CLOSES.fetch_add(1, Relaxed);
        eprintln!("[gauge] close counter#{} (bumped {} times)", self.id, self.n);
    }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__gauge__open() -> RocBox {
    let id = OPENS.fetch_add(1, Relaxed) + 1;
    eprintln!("[gauge] open counter#{id}");
    abi::resource::new(CounterState { id, n: 0 })
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__gauge__bump(c: RocBox) -> u64 {
    let st: &mut CounterState = unsafe { abi::resource::get(c) };
    st.n += 1;
    let n = st.n;
    // Owned-argument contract: this hosted fn owns the reference it received
    // and must release it, or the count never reaches zero (B0 finding).
    unsafe { abi::resource::release(c) };
    n
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__gauge__report() -> i32 {
    let o = OPENS.load(Relaxed); let c = CLOSES.load(Relaxed);
    eprintln!("[gauge] report: opens={o} closes={c} live={}", abi::resource::live());
    c as i32
}
