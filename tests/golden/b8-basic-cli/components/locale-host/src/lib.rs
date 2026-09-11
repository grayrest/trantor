//! roc:locale host (roc-native, P3): BCP-47 tags from LANGUAGE / LC_ALL / LANG.
//! `all` is served as count+at so the Roc shim builds the List(Str) (R-B5).
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::*;
use std::sync::OnceLock;

/// "en_US.UTF-8" -> "en-US"; "C"/"POSIX" -> none.
fn to_tag(raw: &str) -> Option<String> {
    let base = raw.split('.').next().unwrap_or("").split('@').next().unwrap_or("");
    if base.is_empty() || base == "C" || base == "POSIX" { return None; }
    Some(base.replace('_', "-"))
}
fn locales() -> &'static Vec<String> {
    static L: OnceLock<Vec<String>> = OnceLock::new();
    L.get_or_init(|| {
        let mut v = Vec::new();
        if let Ok(list) = std::env::var("LANGUAGE") { v.extend(list.split(':').filter_map(to_tag)); }
        for k in ["LC_ALL", "LANG"] {
            if let Ok(x) = std::env::var(k) { if let Some(t) = to_tag(&x) { if !v.contains(&t) { v.push(t); } } }
        }
        v
    })
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__locale_host__get() -> LocaleHostGetResult {
    match locales().first() {
        Some(t) => LocaleHostGetResult { payload: LocaleHostGetResultPayload { ok: ManuallyDrop::new(RocStr::from_str(t, abi::host())) }, tag: LocaleHostGetResultTag::Ok },
        None => LocaleHostGetResult { payload: LocaleHostGetResultPayload { err: [] }, tag: LocaleHostGetResultTag::Err },
    }
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__locale_host__count() -> u64 { locales().len() as u64 }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__locale_host__at(i: u64) -> RocStr {
    RocStr::from_str(locales().get(i as usize).map(String::as_str).unwrap_or(""), abi::host())
}
