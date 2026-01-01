//! Solver implementations for chaser-cf operations

use super::BrowserManager;
use crate::error::{ChaserError, ChaserResult};
use crate::models::{Cookie, Profile, ProxyConfig, WafSession};

use std::collections::HashMap;
use std::time::Duration;

/// Embedded fake page HTML for turnstile-min mode
const FAKE_PAGE_HTML: &str = include_str!("../resources/fake_page.html");

/// Get page source from a Cloudflare-protected URL
pub async fn get_source(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
    _profile: Profile,
) -> ChaserResult<String> {
    // Acquire context permit
    let _permit = manager.acquire_permit().await?;

    // Create context with proxy if provided
    let ctx_id = manager.create_context(proxy.as_ref()).await?;

    // Create page in context
    let page = manager.new_page_in_context(ctx_id, "about:blank").await?;

    // Navigate and wait for load
    page.goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    // Wait for potential CF challenge to complete
    // We look for a successful response by waiting for the page to stabilize
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Try to detect if we're still on a CF challenge page
    let mut attempts = 0;
    let max_attempts = 30;
    loop {
        let html = page
            .content()
            .await
            .map_err(|e| ChaserError::Internal(e.to_string()))?;

        // Check if we've passed the challenge (no CF challenge indicators)
        if !is_challenge_page(&html) || attempts >= max_attempts {
            return Ok(html);
        }

        attempts += 1;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// Create WAF session with cookies and headers
pub async fn solve_waf_session(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
    _profile: Profile,
) -> ChaserResult<WafSession> {
    // Acquire context permit
    let _permit = manager.acquire_permit().await?;

    // Create context with proxy if provided
    let ctx_id = manager.create_context(proxy.as_ref()).await?;

    // Create page in context
    let page = manager.new_page_in_context(ctx_id, "about:blank").await?;

    // First, get Accept-Language via httpbin
    let accept_language = get_accept_language(&page).await.unwrap_or_default();

    // Navigate to target URL
    page.goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    // Wait for potential CF challenge to complete
    let mut attempts = 0;
    let max_attempts = 30;
    loop {
        let html = page
            .content()
            .await
            .map_err(|e| ChaserError::Internal(e.to_string()))?;

        if !is_challenge_page(&html) || attempts >= max_attempts {
            break;
        }

        attempts += 1;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    // Extract cookies
    let cookies = page
        .get_cookies()
        .await
        .map_err(|e| ChaserError::CookieExtractionFailed(e.to_string()))?;

    let cookies: Vec<Cookie> = cookies
        .into_iter()
        .map(|c| Cookie {
            name: c.name,
            value: c.value,
            domain: Some(c.domain),
            path: Some(c.path),
            expires: Some(c.expires), // Convert f64 to Option<f64>
            http_only: Some(c.http_only),
            secure: Some(c.secure),
            same_site: c.same_site.map(|s| format!("{:?}", s)),
        })
        .collect();

    // Build headers
    let mut headers = HashMap::new();

    // Get user agent from page
    let user_agent = page
        .evaluate("navigator.userAgent")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();

    headers.insert("user-agent".to_string(), user_agent);
    if !accept_language.is_empty() {
        headers.insert("accept-language".to_string(), accept_language);
    }

    Ok(WafSession::new(cookies, headers))
}

/// Solve Turnstile with full page load
pub async fn solve_turnstile_max(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
    _profile: Profile,
) -> ChaserResult<String> {
    // Acquire context permit
    let _permit = manager.acquire_permit().await?;

    // Create context with proxy if provided
    let ctx_id = manager.create_context(proxy.as_ref()).await?;

    // Create page in context (starts at about:blank, stealth already injected)
    let page = manager.new_page_in_context(ctx_id, "about:blank").await?;

    // Inject token extraction script before navigation
    page.evaluate_on_new_document(TURNSTILE_EXTRACTOR_SCRIPT)
        .await
        .map_err(|e| ChaserError::Internal(e.to_string()))?;

    // Navigate to the page with Turnstile
    page.goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    // Wait for the cf-response element to appear
    let token = wait_for_turnstile_token(&page, 60).await?;

    Ok(token)
}

/// Solve Turnstile with minimal resource usage
///
/// This mode uses request interception to serve a lightweight HTML page
/// that only loads the Turnstile widget, avoiding full page resource loading.
pub async fn solve_turnstile_min(
    manager: &BrowserManager,
    url: &str,
    site_key: &str,
    proxy: Option<ProxyConfig>,
    _profile: Profile,
) -> ChaserResult<String> {
    use chaser_oxide::cdp::browser_protocol::fetch::EventRequestPaused;
    use chaser_oxide::cdp::browser_protocol::network::ResourceType;
    use futures::StreamExt;

    // Acquire context permit
    let _permit = manager.acquire_permit().await?;

    // Create context with proxy if provided
    let ctx_id = manager.create_context(proxy.as_ref()).await?;

    // Prepare fake page HTML with site key
    let fake_html = FAKE_PAGE_HTML.replace("<site-key>", site_key);

    // Create page in context
    let page = manager.new_page_in_context(ctx_id, "about:blank").await?;

    // Wrap in ChaserPage for request interception API
    let chaser = chaser_oxide::ChaserPage::new(page.clone());

    // Enable request interception for document requests
    chaser
        .enable_request_interception("*", Some(ResourceType::Document))
        .await
        .map_err(|e| ChaserError::Internal(format!("Failed to enable interception: {}", e)))?;

    // Set up event listener for intercepted requests
    let mut request_events = page
        .event_listener::<EventRequestPaused>()
        .await
        .map_err(|e| ChaserError::Internal(format!("Failed to listen for requests: {}", e)))?;

    // Clone values for the spawned task
    let url_clone = url.to_string();
    let fake_html_clone = fake_html.clone();
    let chaser_clone = chaser.clone();

    // Spawn task to handle intercepted requests
    let intercept_handle = tokio::spawn(async move {
        while let Some(event) = request_events.next().await {
            let request_url = &event.request.url;

            // Check if this is the document request for our target URL
            let is_target = request_url == &url_clone
                || request_url == &format!("{}/", url_clone)
                || request_url.starts_with(&url_clone);

            if is_target && event.resource_type == ResourceType::Document {
                // Fulfill with our minimal Turnstile page
                let _ = chaser_clone
                    .fulfill_request_html(event.request_id.clone(), &fake_html_clone, 200)
                    .await;
            } else {
                // Continue other requests
                let _ = chaser_clone
                    .continue_request(event.request_id.clone())
                    .await;
            }
        }
    });

    // Navigate to the URL (will be intercepted)
    page.goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    // Wait for the cf-response element to appear
    let token = wait_for_turnstile_token(&page, 60).await?;

    // Clean up
    intercept_handle.abort();
    let _ = chaser.disable_request_interception().await;

    Ok(token)
}

/// Script injected to extract Turnstile token
const TURNSTILE_EXTRACTOR_SCRIPT: &str = r#"
    let token = null;
    async function waitForToken() {
        while (!token) {
            try {
                token = window.turnstile.getResponse();
            } catch (e) {}
            await new Promise(resolve => setTimeout(resolve, 500));
        }
        var c = document.createElement("input");
        c.type = "hidden";
        c.name = "cf-response";
        c.value = token;
        document.body.appendChild(c);
    }
    waitForToken();
"#;

/// Wait for Turnstile token to be available
async fn wait_for_turnstile_token(
    page: &chaser_oxide::Page,
    timeout_seconds: u64,
) -> ChaserResult<String> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);

    loop {
        if start.elapsed() > timeout {
            return Err(ChaserError::CaptchaFailed(
                "Timeout waiting for token".to_string(),
            ));
        }

        // Try to get the token from the hidden input
        let result = page
            .evaluate(
                r#"
                (function() {
                    var el = document.querySelector('[name="cf-response"]');
                    return el ? el.value : null;
                })()
            "#,
            )
            .await;

        if let Ok(value) = result {
            if let Ok(token) = value.into_value::<String>() {
                if token.len() > 10 {
                    return Ok(token);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Get Accept-Language header via httpbin
async fn get_accept_language(page: &chaser_oxide::Page) -> Option<String> {
    let result = page
        .evaluate(
            r#"
            fetch("https://httpbin.org/get")
                .then(r => r.json())
                .then(r => r.headers["Accept-Language"] || r.headers["accept-language"])
                .catch(() => null)
        "#,
        )
        .await
        .ok()?;

    result.into_value::<String>().ok()
}

/// Check if page content appears to be a Cloudflare challenge page
fn is_challenge_page(html: &str) -> bool {
    let challenge_indicators = [
        "challenge-platform",
        "cf-spinner",
        "cf_chl_opt",
        "Just a moment",
        "Checking your browser",
        "ray ID",
        "__cf_chl",
    ];

    let html_lower = html.to_lowercase();
    challenge_indicators
        .iter()
        .any(|indicator| html_lower.contains(&indicator.to_lowercase()))
}
