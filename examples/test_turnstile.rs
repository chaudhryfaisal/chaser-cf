//! Test Turnstile solving against winna.com

use chaser_cf::{ChaserCF, ChaserConfig, Profile};

const TARGET_URL: &str = "https://winna.com";
const SITE_KEY: &str = "0x4AAAAAACHcU3E6UUbmv3p-";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_env_filter("chaser_cf=debug,chaser_oxide=debug")
        .init();

    println!("===========================================");
    println!("  chaser-cf - Turnstile Test");
    println!("  Target: {}", TARGET_URL);
    println!("  SiteKey: {}", SITE_KEY);
    println!("===========================================\n");

    // Create config - not headless so we can see what's happening
    let config = ChaserConfig::default()
        .with_profile(Profile::Windows)
        .with_timeout_ms(120000) // 2 minute timeout
        .with_headless(false);

    println!("[1/4] Initializing chaser-cf...");
    let chaser = ChaserCF::new(config).await?;
    println!("[1/4] chaser-cf initialized!\n");

    // Test 1: Turnstile Min (lightweight, uses sitekey)
    println!("[2/4] Testing turnstile-min mode...");
    println!("      URL: {}", TARGET_URL);
    println!("      SiteKey: {}\n", SITE_KEY);

    match chaser.solve_turnstile_min(TARGET_URL, SITE_KEY, None).await {
        Ok(token) => {
            println!("[2/4] SUCCESS! Token received:");
            println!("      Length: {} chars", token.len());
            println!("      Preview: {}...", &token[..token.len().min(50)]);
        }
        Err(e) => {
            println!("[2/4] FAILED: {}", e);
            println!("      Trying turnstile-max mode instead...\n");

            // Test 2: Turnstile Max (full page load)
            println!("[3/4] Testing turnstile-max mode...");
            match chaser.solve_turnstile(TARGET_URL, None).await {
                Ok(token) => {
                    println!("[3/4] SUCCESS! Token received:");
                    println!("      Length: {} chars", token.len());
                    println!("      Preview: {}...", &token[..token.len().min(50)]);
                }
                Err(e) => {
                    println!("[3/4] FAILED: {}", e);
                }
            }
        }
    }

    // Test 3: WAF Session
    println!("\n[4/4] Testing WAF session extraction...");
    match chaser.solve_waf_session(TARGET_URL, None).await {
        Ok(session) => {
            println!("[4/4] SUCCESS! Session received:");
            println!("      Cookies: {}", session.cookies.len());
            for cookie in &session.cookies {
                println!(
                    "        - {}: {}...",
                    cookie.name,
                    &cookie.value[..cookie.value.len().min(20)]
                );
            }
            println!("      Headers: {}", session.headers.len());
            for (k, v) in &session.headers {
                println!("        - {}: {}...", k, &v[..v.len().min(50)]);
            }
        }
        Err(e) => {
            println!("[4/4] FAILED: {}", e);
        }
    }

    println!("\n===========================================");
    println!("  Test Complete!");
    println!("===========================================");

    // Cleanup
    chaser.shutdown().await;

    Ok(())
}
