#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// C-compatible configuration structure
struct ChaserConfigFFI {
  /// Maximum concurrent browser contexts
  uint32_t context_limit;
  /// Request timeout in milliseconds
  uint64_t timeout_ms;
  /// Profile: 0 = Windows, 1 = Linux, 2 = macOS
  uint32_t profile;
  /// Whether to defer browser init (0 = false, 1 = true)
  uint32_t lazy_init;
  /// Whether to run headless (0 = false, 1 = true)
  uint32_t headless;
  /// Chrome binary path (NULL for auto-detect)
  const char *chrome_path;
};

/// C-compatible proxy configuration
struct ProxyConfigFFI {
  /// Proxy host
  const char *host;
  /// Proxy port
  uint16_t port;
  /// Optional username (NULL if none)
  const char *username;
  /// Optional password (NULL if none)
  const char *password;
};

/// Callback type for async operations
///
/// # Parameters
/// - `result`: JSON-encoded result string (must be freed with `chaser_free_string`)
/// - `user_data`: The user_data pointer passed to the async function
using ChaserCallback = void(*)(const char *result, void *user_data);

extern "C" {

/// Get default configuration
ChaserConfigFFI chaser_config_default();

/// Initialize chaser-cf
///
/// # Parameters
/// - `config`: Configuration pointer (NULL for defaults)
///
/// # Returns
/// - 0 on success
/// - Non-zero error code on failure
///
/// # Safety
/// - `config` must be either NULL or a valid pointer to a `ChaserConfigFFI` struct
/// - If `config.chrome_path` is not NULL, it must be a valid null-terminated C string
int32_t chaser_init(const ChaserConfigFFI *config);

/// Shutdown chaser-cf and release resources
void chaser_shutdown();

/// Check if chaser-cf is initialized and ready
///
/// # Returns
/// - 1 if ready
/// - 0 if not ready
int32_t chaser_is_ready();

/// Solve WAF session asynchronously
///
/// # Parameters
/// - `url`: Target URL (null-terminated C string)
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
///
/// # Safety
/// - `url` must be a valid null-terminated C string
/// - `proxy` must be either NULL or a valid pointer to a `ProxyConfigFFI` struct
/// - `callback` must be a valid function pointer
void chaser_solve_waf_async(const char *url,
                            const ProxyConfigFFI *proxy,
                            void *user_data,
                            ChaserCallback callback);

/// Get page source asynchronously
///
/// # Parameters
/// - `url`: Target URL (null-terminated C string)
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
///
/// # Safety
/// - `url` must be a valid null-terminated C string
/// - `proxy` must be either NULL or a valid pointer to a `ProxyConfigFFI` struct
/// - `callback` must be a valid function pointer
void chaser_get_source_async(const char *url,
                             const ProxyConfigFFI *proxy,
                             void *user_data,
                             ChaserCallback callback);

/// Solve Turnstile (full page) asynchronously
///
/// # Parameters
/// - `url`: URL with Turnstile widget
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
///
/// # Safety
/// - `url` must be a valid null-terminated C string
/// - `proxy` must be either NULL or a valid pointer to a `ProxyConfigFFI` struct
/// - `callback` must be a valid function pointer
void chaser_solve_turnstile_async(const char *url,
                                  const ProxyConfigFFI *proxy,
                                  void *user_data,
                                  ChaserCallback callback);

/// Solve Turnstile (minimal page) asynchronously
///
/// # Parameters
/// - `url`: Origin URL for Turnstile
/// - `site_key`: Turnstile site key
/// - `proxy`: Optional proxy config (NULL for no proxy)
/// - `user_data`: User context, returned in callback
/// - `callback`: Function called with JSON result
///
/// # Safety
/// - `url` must be a valid null-terminated C string
/// - `site_key` must be a valid null-terminated C string
/// - `proxy` must be either NULL or a valid pointer to a `ProxyConfigFFI` struct
/// - `callback` must be a valid function pointer
void chaser_solve_turnstile_min_async(const char *url,
                                      const char *site_key,
                                      const ProxyConfigFFI *proxy,
                                      void *user_data,
                                      ChaserCallback callback);

/// Free a string returned by chaser-cf
///
/// Must be called on any string returned via callbacks.
///
/// # Safety
/// - `s` must be either NULL or a valid pointer previously returned by chaser-cf callbacks
/// - `s` must not be freed more than once
void chaser_free_string(char *s);

} // extern "C"
