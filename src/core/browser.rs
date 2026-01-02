//! Browser management for chaser-cf
//!
//! This module handles the browser lifecycle, context pooling, and
//! integration with chaser_oxide.

use crate::error::{ChaserError, ChaserResult};
use crate::models::{Profile, ProxyConfig};

use chaser_oxide::cdp::browser_protocol::browser::BrowserContextId;
use chaser_oxide::cdp::browser_protocol::target::CreateTargetParams;
use chaser_oxide::{Browser, BrowserConfig, ChaserPage, ChaserProfile};
use futures::StreamExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Manages browser lifecycle and context pooling
pub struct BrowserManager {
    /// The browser instance
    browser: Browser,
    /// Semaphore for limiting concurrent contexts
    context_semaphore: Arc<Semaphore>,
    /// Current number of active contexts
    active_contexts: Arc<AtomicUsize>,
    /// Maximum allowed contexts
    max_contexts: usize,
    /// Whether the browser is healthy
    healthy: Arc<AtomicBool>,
    /// The stealth profile for this browser
    profile: ChaserProfile,
}

impl BrowserManager {
    /// Create a new browser manager with the given configuration
    pub async fn new(config: &super::ChaserConfig) -> ChaserResult<Self> {
        tracing::debug!("Creating browser manager with config: {:?}", config);

        // Build chaser_oxide profile based on config
        let profile = match config.profile {
            Profile::Windows => ChaserProfile::windows().build(),
            Profile::Linux => ChaserProfile::linux().build(),
            Profile::Macos => ChaserProfile::macos_arm().build(),
        };

        // Build browser config WITH all stealth settings from profile
        let mut browser_config = BrowserConfig::builder()
            .window_size(profile.screen_width(), profile.screen_height())
            .args(vec![
                "--disable-blink-features=AutomationControlled".to_string(),
                "--disable-infobars".to_string(),
                format!("--window-size={},{}", profile.screen_width(), profile.screen_height()),
            ]);

        // Set Chrome path if specified
        if let Some(ref path) = config.chrome_path {
            browser_config = browser_config.chrome_executable(path.clone());
        }

        // Set headless mode (with_head = NOT headless)
        if !config.headless {
            browser_config = browser_config.with_head();
        }

        let browser_config = browser_config
            .build()
            .map_err(|e| ChaserError::InitFailed(e.to_string()))?;

        // Launch browser
        let (browser, mut handler) = Browser::launch(browser_config)
            .await
            .map_err(|e| ChaserError::InitFailed(e.to_string()))?;

        // Spawn handler task to process browser events
        let healthy = Arc::new(AtomicBool::new(true));
        let healthy_clone = healthy.clone();
        tokio::spawn(async move {
            loop {
                match handler.next().await {
                    Some(_event) => {
                        // Event processed
                    }
                    None => {
                        tracing::warn!("Browser handler ended");
                        healthy_clone.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            browser,
            context_semaphore: Arc::new(Semaphore::new(config.context_limit)),
            active_contexts: Arc::new(AtomicUsize::new(0)),
            max_contexts: config.context_limit,
            healthy,
            profile,
        })
    }

    /// Get the stealth profile
    pub fn profile(&self) -> &ChaserProfile {
        &self.profile
    }

    /// Check if the browser is healthy
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    /// Get current active context count
    pub fn active_contexts(&self) -> usize {
        self.active_contexts.load(Ordering::SeqCst)
    }

    /// Get maximum context limit
    pub fn max_contexts(&self) -> usize {
        self.max_contexts
    }

    /// Acquire a context permit (blocks until available)
    pub async fn acquire_permit(&self) -> ChaserResult<ContextPermit> {
        let permit = self
            .context_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ChaserError::ContextFailed("Semaphore closed".to_string()))?;

        self.active_contexts.fetch_add(1, Ordering::SeqCst);

        Ok(ContextPermit {
            _permit: permit,
            active_contexts: self.active_contexts.clone(),
        })
    }

    /// Try to acquire a context permit (non-blocking)
    pub fn try_acquire_permit(&self) -> Option<ContextPermit> {
        let permit = self.context_semaphore.clone().try_acquire_owned().ok()?;

        self.active_contexts.fetch_add(1, Ordering::SeqCst);

        Some(ContextPermit {
            _permit: permit,
            active_contexts: self.active_contexts.clone(),
        })
    }

    /// Create a new browser context, optionally with a proxy
    pub async fn create_context(
        &self,
        proxy: Option<&ProxyConfig>,
    ) -> ChaserResult<Option<BrowserContextId>> {
        match proxy {
            Some(proxy) => {
                let ctx_id = self
                    .browser
                    .create_incognito_context_with_proxy(proxy.to_url())
                    .await
                    .map_err(|e| ChaserError::ContextFailed(e.to_string()))?;
                Ok(Some(ctx_id))
            }
            None => Ok(None), // Use default browser context
        }
    }

    /// Create a new page, optionally in a specific context, with stealth profile applied
    ///
    /// # Arguments
    /// * `ctx_id` - Optional browser context ID (for proxy isolation)
    /// * `url` - Initial URL for the page (navigated to AFTER profile is applied)
    /// * `profile_override` - Optional profile to use instead of the default
    pub async fn new_page_in_context(
        &self,
        ctx_id: Option<BrowserContextId>,
        url: &str,
        profile_override: Option<&ChaserProfile>,
    ) -> ChaserResult<chaser_oxide::Page> {
        // CRITICAL: Create page with about:blank first, apply profile, THEN navigate
        // The stealth scripts use AddScriptToEvaluateOnNewDocumentParams which only
        // applies to FUTURE document loads, not the current one!
        let mut params = CreateTargetParams::new("about:blank");
        if let Some(id) = ctx_id {
            params.browser_context_id = Some(id);
        }

        let page = self
            .browser
            .new_page(params)
            .await
            .map_err(|e| ChaserError::PageFailed(e.to_string()))?;

        // Use override profile if provided, otherwise use default
        let profile = profile_override.unwrap_or(&self.profile);

        // Wrap in ChaserPage and apply stealth profile BEFORE navigation
        // This registers scripts for all future document loads
        let chaser = ChaserPage::new(page.clone());
        chaser
            .apply_profile(profile)
            .await
            .map_err(|e| ChaserError::PageFailed(format!("Failed to apply profile: {}", e)))?;

        // NOW navigate to the actual URL - stealth scripts will apply
        if url != "about:blank" {
            page.goto(url)
                .await
                .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;
        }

        Ok(page)
    }

    /// Create a new page (uses default context and profile)
    pub async fn new_page(&self, url: &str) -> ChaserResult<chaser_oxide::Page> {
        self.new_page_in_context(None, url, None).await
    }

    /// Create a new page with a specific profile (uses default context)
    pub async fn new_page_with_profile(
        &self,
        url: &str,
        profile: &ChaserProfile,
    ) -> ChaserResult<chaser_oxide::Page> {
        self.new_page_in_context(None, url, Some(profile)).await
    }

    /// Wrap a page in ChaserPage for stealth interactions
    pub fn chaser_page(&self, page: chaser_oxide::Page) -> ChaserPage {
        ChaserPage::new(page)
    }

    /// Build a ChaserProfile from a Profile enum
    pub fn build_profile(profile: Profile) -> ChaserProfile {
        match profile {
            Profile::Windows => ChaserProfile::windows().build(),
            Profile::Linux => ChaserProfile::linux().build(),
            Profile::Macos => ChaserProfile::macos_arm().build(),
        }
    }

    /// Shutdown the browser
    pub async fn shutdown(self) {
        tracing::info!("Shutting down browser manager");
        self.healthy.store(false, Ordering::SeqCst);
        // Browser will be dropped, closing the connection
    }
}

/// RAII guard for context permits
pub struct ContextPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    active_contexts: Arc<AtomicUsize>,
}

impl Drop for ContextPermit {
    fn drop(&mut self) {
        self.active_contexts.fetch_sub(1, Ordering::SeqCst);
    }
}
