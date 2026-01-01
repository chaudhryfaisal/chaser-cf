# chaser-cf

High-performance Cloudflare bypass library with stealth browser automation. Rust-native with C FFI bindings.

## Features

- **WAF Session**: Extract cookies and headers for authenticated requests
- **Turnstile Solver**: Solve Cloudflare Turnstile captchas (min and max modes)
- **Page Source**: Get HTML source from Cloudflare-protected pages
- **Stealth Profiles**: Windows, Linux, macOS fingerprint profiles
- **C FFI Bindings**: Use from Python, Go, Node.js, C/C++, and more
- **Low Memory**: ~50-100MB footprint vs ~500MB+ for Node.js alternatives

## Installation

### As a Rust Library

Add to your `Cargo.toml`:

```toml
[dependencies]
chaser-cf = { git = "https://github.com/ccheshirecat/chaser-cf" }
```

### Building from Source

```bash
git clone https://github.com/ccheshirecat/chaser-cf
cd chaser-cf

# Build library only
cargo build --release

# Build with HTTP server
cargo build --release --features http-server

# Generate C headers
cargo build --release  # Headers generated to include/chaser_cf.h
```

### Docker

```bash
docker build -t chaser-cf .
docker run -d -p 3000:3000 chaser-cf
```

## Usage

### Rust

```rust
use chaser_cf::{ChaserCF, ChaserConfig, Profile};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize with default config
    let chaser = ChaserCF::new(ChaserConfig::default()).await?;

    // Get page source from CF-protected site
    let source = chaser.get_source("https://example.com", None).await?;
    println!("Got {} bytes", source.len());

    // Create WAF session (cookies + headers)
    let session = chaser.solve_waf_session("https://example.com", None).await?;
    println!("Cookies: {}", session.cookies_string());

    // Solve Turnstile (full page)
    let token = chaser.solve_turnstile("https://example.com/captcha", None).await?;
    println!("Token: {}", token);

    // Solve Turnstile (minimal resources)
    let token = chaser.solve_turnstile_min(
        "https://example.com",
        "0x4AAAAAAxxxxx",  // site key
        None
    ).await?;

    chaser.shutdown().await;
    Ok(())
}
```

### C/C++

```c
#include "chaser_cf.h"
#include <stdio.h>
#include <unistd.h>

void on_result(const char* json_result, void* ctx) {
    printf("Result: %s\n", json_result);
    chaser_free_string((char*)json_result);
}

int main() {
    // Initialize with default config
    ChaserConfig config = chaser_config_default();
    int err = chaser_init(&config);
    if (err != 0) {
        printf("Init failed: %d\n", err);
        return 1;
    }

    // Solve WAF session
    chaser_solve_waf_async("https://example.com", NULL, NULL, on_result);
    
    // Wait for callback
    sleep(30);

    chaser_shutdown();
    return 0;
}
```

Compile with:
```bash
gcc -o example example.c -L./target/release -lchaser_cf -lpthread -ldl -lm
```

### Python (via ctypes)

```python
import ctypes
import json
from ctypes import c_char_p, c_void_p, c_int, CFUNCTYPE

# Load library
lib = ctypes.CDLL('./target/release/libchaser_cf.so')

# Define callback type
CALLBACK = CFUNCTYPE(None, c_char_p, c_void_p)

results = []

@CALLBACK
def on_result(json_result, user_data):
    result = json.loads(json_result.decode())
    results.append(result)
    lib.chaser_free_string(json_result)

# Initialize
lib.chaser_init(None)

# Solve WAF
lib.chaser_solve_waf_async(b"https://example.com", None, None, on_result)

# Wait for result
import time
while not results:
    time.sleep(0.1)

print(results[0])

lib.chaser_shutdown()
```

### HTTP Server

```bash
# Run server
cargo run --release --features http-server --bin chaser-cf-server

# Or with Docker
docker run -d -p 3000:3000 \
  -e PORT=3000 \
  -e CHASER_CONTEXT_LIMIT=20 \
  -e CHASER_TIMEOUT=60000 \
  chaser-cf
```

API endpoints:

```bash
# Get page source
curl -X POST http://localhost:3000/solve \
  -H "Content-Type: application/json" \
  -d '{"mode": "source", "url": "https://example.com"}'

# Create WAF session
curl -X POST http://localhost:3000/solve \
  -H "Content-Type: application/json" \
  -d '{"mode": "waf-session", "url": "https://example.com"}'

# Solve Turnstile (full page)
curl -X POST http://localhost:3000/solve \
  -H "Content-Type: application/json" \
  -d '{"mode": "turnstile-max", "url": "https://example.com/captcha"}'

# Solve Turnstile (minimal)
curl -X POST http://localhost:3000/solve \
  -H "Content-Type: application/json" \
  -d '{"mode": "turnstile-min", "url": "https://example.com", "siteKey": "0x4AAA..."}'
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3000` | HTTP server port |
| `CHASER_CONTEXT_LIMIT` | `20` | Max concurrent browser contexts |
| `CHASER_TIMEOUT` | `60000` | Request timeout (ms) |
| `CHASER_PROFILE` | `windows` | Stealth profile (windows/linux/macos) |
| `CHASER_LAZY_INIT` | `false` | Defer browser init until first use |
| `CHASER_HEADLESS` | `false` | Run browser headless |
| `CHROME_BIN` | auto-detect | Path to Chrome/Chromium binary |
| `AUTH_TOKEN` | none | Optional API auth token |

### Rust Config

```rust
let config = ChaserConfig::default()
    .with_context_limit(10)
    .with_timeout_ms(30000)
    .with_profile(Profile::Linux)
    .with_lazy_init(true)
    .with_headless(false)
    .with_chrome_path("/usr/bin/chromium");
```

## API Reference

### Rust API

```rust
impl ChaserCF {
    async fn new(config: ChaserConfig) -> ChaserResult<Self>;
    async fn init(&self) -> ChaserResult<()>;
    async fn shutdown(&self);
    async fn is_ready(&self) -> bool;
    
    async fn get_source(&self, url: &str, proxy: Option<ProxyConfig>) -> ChaserResult<String>;
    async fn solve_waf_session(&self, url: &str, proxy: Option<ProxyConfig>) -> ChaserResult<WafSession>;
    async fn solve_turnstile(&self, url: &str, proxy: Option<ProxyConfig>) -> ChaserResult<String>;
    async fn solve_turnstile_min(&self, url: &str, site_key: &str, proxy: Option<ProxyConfig>) -> ChaserResult<String>;
}
```

### C FFI API

```c
// Initialization
ChaserConfig chaser_config_default(void);
int chaser_init(const ChaserConfig* config);
void chaser_shutdown(void);
int chaser_is_ready(void);

// Async operations (callback-based)
void chaser_solve_waf_async(const char* url, const ProxyConfig* proxy, void* user_data, ChaserCallback callback);
void chaser_get_source_async(const char* url, const ProxyConfig* proxy, void* user_data, ChaserCallback callback);
void chaser_solve_turnstile_async(const char* url, const ProxyConfig* proxy, void* user_data, ChaserCallback callback);
void chaser_solve_turnstile_min_async(const char* url, const char* site_key, const ProxyConfig* proxy, void* user_data, ChaserCallback callback);

// Memory management
void chaser_free_string(char* s);
```

## Response Format

All operations return JSON:

```json
// Success - WAF Session
{
  "type": "WafSession",
  "data": {
    "cookies": [{"name": "cf_clearance", "value": "..."}],
    "headers": {"user-agent": "..."}
  }
}

// Success - Token
{
  "type": "Token",
  "data": "0.abc123..."
}

// Success - Source
{
  "type": "Source", 
  "data": "<html>..."
}

// Error
{
  "type": "Error",
  "data": {"code": 6, "message": "Operation timed out after 60000ms"}
}
```

## Dependencies

- [chaser_oxide](https://github.com/ccheshirecat/chaser-oxide) - Stealth browser automation (fork of chromiumoxide)
- Chrome/Chromium browser installed on system

## License

MIT OR Apache-2.0

## Acknowledgements

- [chromiumoxide](https://github.com/mattsse/chromiumoxide) - Base CDP client
- [puppeteer-real-browser](https://github.com/nickvicious/puppeteer-real-browser) - Stealth technique inspiration
- [cf-clearance-scraper](https://github.com/zfcsoftware/cf-clearance-scraper) - Original Node.js implementation
