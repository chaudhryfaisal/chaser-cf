//! chaser-cf CLI Example
//!
//! This is a simple example showing how to use chaser-cf from Rust.
//! For the HTTP server, use `cargo run --features http-server --bin chaser-cf-server`

use chaser_cf::{ChaserCF, ChaserConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("chaser_cf=info")
        .init();

    println!("chaser-cf - Cloudflare Bypass Library");
    println!("========================================\n");

    // Create config
    let config = ChaserConfig::default()
        .with_lazy_init(false)
        .with_timeout_ms(60000);

    println!("Initializing chaser-cf...");
    let chaser = ChaserCF::new(config).await?;
    println!("chaser-cf initialized!\n");

    // Example: Get page source
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://example.com".to_string());

    println!("Fetching source from: {}", url);
    match chaser.get_source(&url, None).await {
        Ok(source) => {
            println!("Got {} bytes of HTML", source.len());
            println!("First 500 chars:\n{}", &source[..source.len().min(500)]);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    // Clean shutdown
    chaser.shutdown().await;
    println!("\nchaser-cf shutdown complete");

    Ok(())
}
