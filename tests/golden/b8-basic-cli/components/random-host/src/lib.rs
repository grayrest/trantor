//! roc:random host: OS entropy seeds (/dev/urandom; no crate dep).
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::*;
use std::io::Read;

fn entropy(buf: &mut [u8]) -> std::io::Result<()> { std::fs::File::open("/dev/urandom")?.read_exact(buf) }

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__random_host__seed_u64() -> RandomHostSeedU64Result {
    let mut b = [0u8; 8];
    match entropy(&mut b) {
        Ok(()) => RandomHostSeedU64Result { payload: RandomHostSeedU64ResultPayload { ok: ManuallyDrop::new(u64::from_le_bytes(b)) }, tag: RandomHostSeedU64ResultTag::Ok },
        Err(e) => RandomHostSeedU64Result { payload: RandomHostSeedU64ResultPayload { err: ManuallyDrop::new(RandomHostIOErr { payload: RandomHostIOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag: RandomHostIOErrTag::Other }) }, tag: RandomHostSeedU64ResultTag::Err },
    }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__random_host__seed_u32() -> RandomHostSeedU32Result {
    let mut b = [0u8; 4];
    match entropy(&mut b) {
        Ok(()) => RandomHostSeedU32Result { payload: RandomHostSeedU32ResultPayload { ok: ManuallyDrop::new(u32::from_le_bytes(b)) }, tag: RandomHostSeedU32ResultTag::Ok },
        Err(e) => RandomHostSeedU32Result { payload: RandomHostSeedU32ResultPayload { err: ManuallyDrop::new(IOErr { payload: IOErrPayload { other: ManuallyDrop::new(RocStr::from_str(&e.to_string(), abi::host())) }, tag: IOErrTag::Other }) }, tag: RandomHostSeedU32ResultTag::Err },
    }
}
