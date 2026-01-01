//! Configuration for chaser-cf

use crate::models::Profile;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for ChaserCF
#[derive(Debug, Clone)]
pub struct ChaserConfig {
    /// Maximum concurrent browser contexts (default: 20)
    pub context_limit: usize,

    /// Request timeout in milliseconds (default: 60000)
    pub timeout_ms: u64,

    /// Stealth profile to use (default: Windows)
    pub profile: Profile,

    /// Whether to defer browser initialization until first use (default: false)
    pub lazy_init: bool,

    /// Path to Chrome/Chromium binary (default: auto-detect)
    pub chrome_path: Option<PathBuf>,

    /// Whether to run in headless mode (default: false for stealth)
    pub headless: bool,

    /// Browser viewport width (default: 1920)
    pub viewport_width: u32,

    /// Browser viewport height (default: 1080)
    pub viewport_height: u32,
}

impl Default for ChaserConfig {
    fn default() -> Self {
        Self {
            context_limit: 20,
            timeout_ms: 60000,
            profile: Profile::Windows,
            lazy_init: false,
            chrome_path: None,
            headless: false,
            viewport_width: 1920,
            viewport_height: 1080,
        }
    }
}

impl ChaserConfig {
    /// Create configuration from environment variables
    ///
    /// Environment variables:
    /// - `CHASER_CONTEXT_LIMIT`: Max concurrent contexts (default: 20)
    /// - `CHASER_TIMEOUT`: Timeout in ms (default: 60000)
    /// - `CHASER_PROFILE`: Profile name (windows/linux/macos)
    /// - `CHASER_LAZY_INIT`: Enable lazy init (true/false)
    /// - `CHROME_BIN`: Path to Chrome binary
    /// - `CHASER_HEADLESS`: Run headless (true/false)
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = env::var("CHASER_CONTEXT_LIMIT") {
            if let Ok(limit) = val.parse() {
                config.context_limit = limit;
            }
        }

        if let Ok(val) = env::var("CHASER_TIMEOUT") {
            if let Ok(timeout) = val.parse() {
                config.timeout_ms = timeout;
            }
        }

        if let Ok(val) = env::var("CHASER_PROFILE") {
            if let Some(profile) = Profile::from_str(&val) {
                config.profile = profile;
            }
        }

        if let Ok(val) = env::var("CHASER_LAZY_INIT") {
            config.lazy_init = val.eq_ignore_ascii_case("true") || val == "1";
        }

        if let Ok(val) = env::var("CHROME_BIN") {
            config.chrome_path = Some(PathBuf::from(val));
        }

        if let Ok(val) = env::var("CHASER_HEADLESS") {
            config.headless = val.eq_ignore_ascii_case("true") || val == "1";
        }

        config
    }

    /// Builder method: set context limit
    pub fn with_context_limit(mut self, limit: usize) -> Self {
        self.context_limit = limit;
        self
    }

    /// Builder method: set timeout
    pub fn with_timeout_ms(mut self, timeout: u64) -> Self {
        self.timeout_ms = timeout;
        self
    }

    /// Builder method: set timeout from Duration
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_ms = timeout.as_millis() as u64;
        self
    }

    /// Builder method: set profile
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
        self
    }

    /// Builder method: enable lazy initialization
    pub fn with_lazy_init(mut self, lazy: bool) -> Self {
        self.lazy_init = lazy;
        self
    }

    /// Builder method: set Chrome path
    pub fn with_chrome_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.chrome_path = Some(path.into());
        self
    }

    /// Builder method: set headless mode
    pub fn with_headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Builder method: set viewport size
    pub fn with_viewport(mut self, width: u32, height: u32) -> Self {
        self.viewport_width = width;
        self.viewport_height = height;
        self
    }

    /// Get timeout as Duration
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ChaserConfig::default();
        assert_eq!(config.context_limit, 20);
        assert_eq!(config.timeout_ms, 60000);
        assert_eq!(config.profile, Profile::Windows);
        assert!(!config.lazy_init);
        assert!(!config.headless);
    }

    #[test]
    fn test_builder_pattern() {
        let config = ChaserConfig::default()
            .with_context_limit(10)
            .with_timeout_ms(30000)
            .with_profile(Profile::Linux)
            .with_lazy_init(true)
            .with_headless(true);

        assert_eq!(config.context_limit, 10);
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.profile, Profile::Linux);
        assert!(config.lazy_init);
        assert!(config.headless);
    }
}
