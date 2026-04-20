use chaser_cf::{ChaserCF, ChaserConfig, ProxyConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("chaser_cf=debug,chaser_oxide=info")
        .init();

    let config = ChaserConfig::default().with_headless(true);
    let chaser = ChaserCF::new(config).await?;

    let proxy = Some(ProxyConfig::new("proxy.chaser.sh".to_string(), 10002)
        .with_auth("us_t9cpj3-session-debugXXX".to_string(), "IUP9Iny3HPmKYHFF".to_string()));

    let url = "https://2captcha.com/demo/cloudflare-turnstile-challenge";

    println!("Navigating to {url}");

    // Use internal API via solve_waf_session but patch to inspect
    // Instead, use get_source to see the HTML
    match chaser.get_source(url, proxy).await {
        Ok(html) => {
            // Check for turnstile elements
            println!("=== Page HTML snippet (2000 chars) ===");
            println!("{}", &html[..html.len().min(2000)]);
            println!("...");
            if html.contains("cf-turnstile") {
                println!("\nFOUND: cf-turnstile in page");
            } else {
                println!("\nNOT FOUND: cf-turnstile in page");
            }
            if html.contains("cf_chl_rc_ni") {
                println!("FOUND: cf_chl_rc_ni in page");
            }
            if html.contains("challenge") {
                println!("FOUND: 'challenge' in page");
            }
        }
        Err(e) => println!("Error: {e}"),
    }

    chaser.shutdown().await;
    Ok(())
}
