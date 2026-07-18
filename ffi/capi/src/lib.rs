//! Stable C ABI over relic-core (PLAN.md §4.1).
//!
//! Phase-1 will grow this into the full command/query + JSON-event surface.
//! Contract rules from the plan:
//! - Handles are opaque pointers; every function is `extern "C"` and
//!   panic-free (`catch_unwind` at the boundary).
//! - Structured payloads cross as UTF-8 JSON; events arrive on a registered
//!   callback pointer.
//! - Kotlin/Swift shells use the UniFFI bindings in `ffi/uniffi` instead.

use std::ffi::CStr;
use std::os::raw::c_char;

/// Returns the engine version as a static NUL-terminated string.
#[no_mangle]
pub extern "C" fn relic_version() -> *const c_char {
    // SAFETY: string literal with explicit NUL, 'static lifetime.
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

/// Smoke-test entry point used by shell build systems to verify linkage:
/// opens an in-memory engine and returns the number of built-in systems,
/// or -1 on failure. `_config_json` is reserved (pass NULL).
#[no_mangle]
pub extern "C" fn relic_selftest(_config_json: *const c_char) -> i32 {
    let result = std::panic::catch_unwind(|| {
        let engine = relic_core::api::Engine::open_in_memory().ok()?;
        engine.list_systems().ok().map(|s| s.len() as i32)
    });
    match result {
        Ok(Some(n)) => n,
        _ => -1,
    }
}

/// Helper for future string-taking entry points.
#[allow(dead_code)]
unsafe fn cstr<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selftest_reports_builtin_system_count() {
        assert!(relic_selftest(std::ptr::null()) >= 8);
    }
}
