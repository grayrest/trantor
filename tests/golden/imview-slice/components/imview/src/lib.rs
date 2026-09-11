//! imview driver HOST (hand-authored — a reactor driver ships its own host;
//! trantor generates only the runtime symbols for a CLI driver, so this slice
//! documents that a non-trivial driver provides driver_main itself). It calls
//! the app's roc_im_init/roc_im_view across the boundary and renders the
//! recursive Element tree to text (headless -- no wgpu). Every boundary is
//! extern "C-unwind" (H0d).
#![allow(dead_code)]
use core::ffi::c_void;
use core::ptr;
use trantor_abi as abi;
use abi::{Env, Element, ElementTag, RocBox, AnonStruct74775b1f87f9c7ed as View};

#[unsafe(no_mangle)] pub extern "C" fn roc_alloc(l: usize, a: usize) -> *mut c_void { abi::DefaultAllocators::roc_alloc(ptr::null_mut(), l, a) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_dealloc(p: *mut c_void, a: usize) { abi::DefaultAllocators::roc_dealloc(ptr::null_mut(), p, a) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_realloc(p: *mut c_void, n: usize, a: usize) -> *mut c_void { abi::DefaultAllocators::roc_realloc(ptr::null_mut(), p, n, a) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_dbg(b: *const u8, n: usize) { eprintln!("[dbg] {}", String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(b, n) })); }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_expect_failed(b: *const u8, n: usize) { eprintln!("[expect] {}", String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(b, n) })); }
#[unsafe(no_mangle)] pub unsafe extern "C" fn roc_crashed(b: *const u8, n: usize) { eprintln!("[CRASH] {}", String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(b, n) })); std::process::exit(1); }

unsafe extern "C-unwind" {
    fn roc_im_init(env: Env) -> RocBox;
    fn roc_im_view(model: RocBox) -> View;
}

/// Render the recursive Element tree to a bracketed text form.
fn render(el: &Element) -> String {
    match el.tag {
        ElementTag::Text => unsafe { el.borrow_payload_text_unchecked().as_str().to_string() },
        ElementTag::Row => {
            let kids = unsafe { el.borrow_payload_row_unchecked() };
            let inner: Vec<String> = kids.as_slice().iter().map(render).collect();
            format!("[{}]", inner.join(" | "))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let outcome = std::panic::catch_unwind(|| {
        let env = Env { width: 80 };
        let model = unsafe { roc_im_init(env) };        // app builds its Model (opaque box)
        let view = unsafe { roc_im_view(model) };        // app renders -> { tree: Element }
        println!("{}", render(&view.tree));
    });
    if outcome.is_err() { eprintln!("[trantor] driver caught a panic"); return 70; }
    0
}
