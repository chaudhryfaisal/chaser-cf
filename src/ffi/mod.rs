//! C FFI bindings for chaser-cf
//!
//! This module provides a C-compatible API for using chaser-cf from
//! other languages (Python, Go, Node.js, C/C++, etc.).
//!
//! # Async Model
//!
//! Operations are callback-based. Each async function takes:
//! - Required parameters (URL, etc.)
//! - `user_data: *mut c_void` - Caller's context, returned untouched in callback
//! - `callback: ChaserCallback` - Function called with result
//!
//! # Thread Safety
//!
//! The library maintains an internal Tokio runtime. All operations are thread-safe
//! and can be called from any thread.
//!
//! # Memory Management
//!
//! - Strings returned to C (via callbacks) must be freed with `chaser_free_string`
//! - The library does not take ownership of `user_data`
//!
//! # Example (C)
//!
//! ```c
//! #include "chaser_cf.h"
//!
//! void on_result(const char* json_result, void* ctx) {
//!     printf("Result: %s\n", json_result);
//!     chaser_free_string((char*)json_result);
//! }
//!
//! int main() {
//!     ChaserConfig config = chaser_config_default();
//!     int err = chaser_init(&config);
//!     if (err != 0) return 1;
//!
//!     chaser_solve_waf_async("https://example.com", NULL, NULL, on_result);
//!
//!     // Wait for callback...
//!     sleep(10);
//!
//!     chaser_shutdown();
//!     return 0;
//! }
//! ```

use crate::core::{ChaserCF, ChaserConfig};
use crate::error::ChaserError;
use crate::models::{ChaserResult as ChaserResultModel, Profile, ProxyConfig};

use once_cell::sync::OnceCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

/// Global Tokio runtime for async operations
static RUNTIME: OnceCell<Runtime> = OnceCell::new();

/// Global ChaserCF instance
static CHASER: OnceCell<Arc<RwLock<Option<ChaserCF>>>> = OnceCell::new();

/// Callback type for async operations
///
/// # Parameters
/// - `result`: JSON-encoded result string (must be freed with `chaser_free_string`)
/// - `user_data`: The user_data pointer passed to the async function
pub type ChaserCallback = extern "C" fn(result: *const c_char, user_data: *mut c_void);

/// C-compatible configuration structure
#[repr(C)]
pub struct ChaserConfigFFI {
    /// Maximum concurrent browser contexts
    pub context_limit: u32,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Profile: 0 = Windows, 1 = Linux, 2 = macOS
    pub profile: u32,
    /// Whether to defer browser init (0 = false, 1 = true)
    pub lazy_init: u32,
    /// Whether to run headless (0 = false, 1 = true)
    pub headless: u32,
    /// Chrome binary path (NULL for auto-detect)
    pub chrome_path: *const c_char,
}

/// C-compatible proxy configuration
#[repr(C)]
pub struct ProxyConfigFFI {
    /// Proxy host
    pub host: *const c_char,
    /// Proxy port
    pub port: u16,
    /// Optional username (NULL if none)
    pub username: *const c_char,
    /// Optional password (NULL if none)
    pub password: *const c_char,
}

/// Get default configuration
#[no_mangle]
pub extern "C" fn chaser_config_default() -> ChaserConfigFFI {
    ChaserConfigFFI {
        context_limit: 20,
        timeout_ms: 60000,
        profile: 0, // Windows
        lazy_init: 0,
        headless: 0,
        chrome_path: ptr::null(),
    }
}

/// Initialize chaser-cf
///
/// # Parameters
/// - `config`: Configuration pointer (NULL for defaults)
///
/// # Returns
/// - 0 on success
/// - Non-zero error code on failure
#[no_mangle]
pub extern "C" fn chaser_init(config: *const ChaserConfigFFI) -> i32 {
    // Initialize runtime
    let runtime = RUNTIME.get_or_try_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
    });

    let runtime = match runtime {
        Ok(rt) => rt,
        Err(_) => return -1,
    };

    // Parse config
    let chaser_config = if config.is_null() {
        ChaserConfig::default()
    } else {
        unsafe { config_from_ffi(&*config) }
    };

    // Initialize ChaserCF
    let result = runtime.block_on(async {
        let suite = ChaserCF::new(chaser_config).await?;
        Ok::<_, ChaserError>(suite)
    });

    match result {
        Ok(suite) => {
            let _ = CHASER.set(Arc::new(RwLock::new(Some(suite))));
            0
        }
        Err(e) => e.code(),
    }
}

/// Shutdown chaser-cf and release resources
#[no_mangle]
pub extern "C" fn chaser_shutdown() {
    if let Some(runtime) = RUNTIME.get() {
        if let Some(chaser) = CHASER.get() {
            runtime.block_on(async {
                let mut guard = chaser.write().await;
                if let Some(suite) = guard.take() {
                    suite.shutdown().await;
                }
            });
        }
    }
}

/// Check if chaser-cf is initialized and ready
///
/// # Returns
/// - 1 if ready
/// - 0 if not ready
#[no_mangle]
pub extern "C" fn chaser_is_ready() -> i32 {
    if let (Some(runtime), Some(chaser)) = (RUNTIME.get(), CHASER.get()) {
        let ready = runtime.block_on(async {
            let guard = chaser.read().await;
            guard.as_ref().map(|s| s.is_ready()).unwrap_or(false).await
        });
        if ready {
            1
        } else {
            0
        }
    } else {
        0
    }
}

/// Solve WAF session asynchronously
///
/// # Parameters
/// - `url`: Target URL (null-terminated C string)
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
#[no_mangle]
pub unsafe extern "C" fn chaser_solve_waf_async(
    url: *const c_char,
    proxy: *const ProxyConfigFFI,
    user_data: *mut c_void,
    callback: ChaserCallback,
) {
    let url = match cstr_to_string(url) {
        Some(s) => s,
        None => {
            let result = make_error_json(ChaserError::InvalidUrl("NULL url".to_string()));
            callback(result, user_data);
            return;
        }
    };

    let proxy = proxy_from_ffi(proxy);

    spawn_async_operation(
        user_data,
        callback,
        AsyncOp {
            url,
            site_key: None,
            proxy,
            op_type: OpType::WafSession,
        },
    );
}

/// Get page source asynchronously
///
/// # Parameters
/// - `url`: Target URL (null-terminated C string)
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
#[no_mangle]
pub unsafe extern "C" fn chaser_get_source_async(
    url: *const c_char,
    proxy: *const ProxyConfigFFI,
    user_data: *mut c_void,
    callback: ChaserCallback,
) {
    let url = match cstr_to_string(url) {
        Some(s) => s,
        None => {
            let result = make_error_json(ChaserError::InvalidUrl("NULL url".to_string()));
            callback(result, user_data);
            return;
        }
    };

    let proxy = proxy_from_ffi(proxy);

    spawn_async_operation(
        user_data,
        callback,
        AsyncOp {
            url,
            site_key: None,
            proxy,
            op_type: OpType::Source,
        },
    );
}

/// Solve Turnstile (full page) asynchronously
///
/// # Parameters
/// - `url`: URL with Turnstile widget
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
#[no_mangle]
pub unsafe extern "C" fn chaser_solve_turnstile_async(
    url: *const c_char,
    proxy: *const ProxyConfigFFI,
    user_data: *mut c_void,
    callback: ChaserCallback,
) {
    let url = match cstr_to_string(url) {
        Some(s) => s,
        None => {
            let result = make_error_json(ChaserError::InvalidUrl("NULL url".to_string()));
            callback(result, user_data);
            return;
        }
    };

    let proxy = proxy_from_ffi(proxy);

    spawn_async_operation(
        user_data,
        callback,
        AsyncOp {
            url,
            site_key: None,
            proxy,
            op_type: OpType::Turnstile,
        },
    );
}

/// Solve Turnstile (minimal page) asynchronously
///
/// # Parameters
/// - `url`: Origin URL for Turnstile
/// - `site_key`: Turnstile site key
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
#[no_mangle]
pub unsafe extern "C" fn chaser_solve_turnstile_min_async(
    url: *const c_char,
    site_key: *const c_char,
    proxy: *const ProxyConfigFFI,
    user_data: *mut c_void,
    callback: ChaserCallback,
) {
    let url = match cstr_to_string(url) {
        Some(s) => s,
        None => {
            let result = make_error_json(ChaserError::InvalidUrl("NULL url".to_string()));
            callback(result, user_data);
            return;
        }
    };

    let site_key = match cstr_to_string(site_key) {
        Some(s) => s,
        None => {
            let result = make_error_json(ChaserError::MissingParameter("site_key".to_string()));
            callback(result, user_data);
            return;
        }
    };

    let proxy = proxy_from_ffi(proxy);

    spawn_async_operation(
        user_data,
        callback,
        AsyncOp {
            url,
            site_key: Some(site_key),
            proxy,
            op_type: OpType::TurnstileMin,
        },
    );
}

/// Free a string returned by chaser-cf
///
/// Must be called on any string returned via callbacks.
#[no_mangle]
pub unsafe extern "C" fn chaser_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Convert C string to Rust String
unsafe fn cstr_to_string(s: *const c_char) -> Option<String> {
    if s.is_null() {
        return None;
    }
    CStr::from_ptr(s).to_str().ok().map(|s| s.to_owned())
}

/// Convert FFI config to Rust config
unsafe fn config_from_ffi(ffi: &ChaserConfigFFI) -> ChaserConfig {
    let mut config = ChaserConfig::default()
        .with_context_limit(ffi.context_limit as usize)
        .with_timeout_ms(ffi.timeout_ms)
        .with_lazy_init(ffi.lazy_init != 0)
        .with_headless(ffi.headless != 0);

    config.profile = match ffi.profile {
        1 => Profile::Linux,
        2 => Profile::Macos,
        _ => Profile::Windows,
    };

    if !ffi.chrome_path.is_null() {
        if let Some(path) = cstr_to_string(ffi.chrome_path) {
            config = config.with_chrome_path(path);
        }
    }

    config
}

/// Convert FFI proxy to Rust proxy
unsafe fn proxy_from_ffi(ffi: *const ProxyConfigFFI) -> Option<ProxyConfig> {
    if ffi.is_null() {
        return None;
    }

    let ffi = &*ffi;
    let host = cstr_to_string(ffi.host)?;

    let mut proxy = ProxyConfig::new(host, ffi.port);

    if let (Some(username), Some(password)) =
        (cstr_to_string(ffi.username), cstr_to_string(ffi.password))
    {
        proxy = proxy.with_auth(username, password);
    }

    Some(proxy)
}

/// Make error JSON string
fn make_error_json(error: ChaserError) -> *const c_char {
    let result = ChaserResultModel::error(error.code(), error.to_string());
    let json = serde_json::to_string(&result).unwrap_or_else(|_| {
        r#"{"type":"Error","data":{"code":99,"message":"Serialization failed"}}"#.to_string()
    });
    CString::new(json).unwrap().into_raw()
}

/// Wrapper struct for async operations that captures all needed data
struct AsyncOp {
    url: String,
    site_key: Option<String>,
    proxy: Option<ProxyConfig>,
    op_type: OpType,
}

#[derive(Clone, Copy)]
enum OpType {
    WafSession,
    Source,
    Turnstile,
    TurnstileMin,
}

/// Spawn an async operation on the runtime
fn spawn_async_operation(user_data: *mut c_void, callback: ChaserCallback, op: AsyncOp) {
    let user_data = user_data as usize; // Convert to usize for Send

    let runtime = match RUNTIME.get() {
        Some(rt) => rt,
        None => {
            let result = make_error_json(ChaserError::NotInitialized);
            callback(result, user_data as *mut c_void);
            return;
        }
    };

    let chaser = match CHASER.get() {
        Some(g) => g.clone(),
        None => {
            let result = make_error_json(ChaserError::NotInitialized);
            callback(result, user_data as *mut c_void);
            return;
        }
    };

    runtime.spawn(async move {
        let result = {
            let guard = chaser.read().await;
            match guard.as_ref() {
                Some(suite) => match op.op_type {
                    OpType::WafSession => match suite.solve_waf_session(&op.url, op.proxy).await {
                        Ok(session) => ChaserResultModel::waf_session(session),
                        Err(e) => ChaserResultModel::error(e.code(), e.to_string()),
                    },
                    OpType::Source => match suite.get_source(&op.url, op.proxy).await {
                        Ok(source) => ChaserResultModel::source(source),
                        Err(e) => ChaserResultModel::error(e.code(), e.to_string()),
                    },
                    OpType::Turnstile => match suite.solve_turnstile(&op.url, op.proxy).await {
                        Ok(token) => ChaserResultModel::token(token),
                        Err(e) => ChaserResultModel::error(e.code(), e.to_string()),
                    },
                    OpType::TurnstileMin => {
                        let site_key = op.site_key.as_deref().unwrap_or("");
                        match suite.solve_turnstile_min(&op.url, site_key, op.proxy).await {
                            Ok(token) => ChaserResultModel::token(token),
                            Err(e) => ChaserResultModel::error(e.code(), e.to_string()),
                        }
                    }
                },
                None => ChaserResultModel::error(
                    ChaserError::NotInitialized.code(),
                    "chaser-cf not initialized",
                ),
            }
        };

        let json = serde_json::to_string(&result).unwrap_or_else(|_| {
            r#"{"type":"Error","data":{"code":99,"message":"Serialization failed"}}"#.to_string()
        });
        let c_result = CString::new(json).unwrap().into_raw();

        callback(c_result, user_data as *mut c_void);
    });
}
