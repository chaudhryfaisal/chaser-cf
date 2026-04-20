use chaser_cf::{ChaserConfig, core::BrowserManager};
use std::env;

const FP_SCRIPT: &str = r#"JSON.stringify({
    userAgent:           navigator.userAgent,
    platform:            navigator.platform,
    hardwareConcurrency: navigator.hardwareConcurrency,
    deviceMemory:        navigator.deviceMemory,
    maxTouchPoints:      navigator.maxTouchPoints,
    webdriver:           navigator.webdriver,
    pluginsLength:       navigator.plugins.length,
    pluginNames:         Array.from(navigator.plugins).map(p => p.name),
    mimeTypesLength:     navigator.mimeTypes.length,
    languages:           navigator.languages,
    screen: {
        width:       screen.width,
        height:      screen.height,
        availWidth:  screen.availWidth,
        availHeight: screen.availHeight,
        colorDepth:  screen.colorDepth,
    },
    outerWidth:    window.outerWidth,
    outerHeight:   window.outerHeight,
    innerWidth:    window.innerWidth,
    innerHeight:   window.innerHeight,
    devicePixelRatio: window.devicePixelRatio,
    hasFocus:      document.hasFocus(),
    visibilityState: document.visibilityState,
    chromeRuntime: !!(window.chrome && window.chrome.runtime),
    notificationPermission: (() => { try { return Notification.permission; } catch(e) { return 'N/A'; } })(),
    uaData: (() => {
        try {
            const d = navigator.userAgentData;
            return d ? { platform: d.platform, mobile: d.mobile, brands: d.brands } : null;
        } catch(e) { return null; }
    })(),
    webglVendor: (() => {
        try {
            const g = document.createElement('canvas').getContext('webgl');
            if (!g) return 'no webgl context';
            const ext = g.getExtension('WEBGL_debug_renderer_info');
            return g.getParameter(ext ? ext.UNMASKED_VENDOR_WEBGL : 37445);
        } catch(e) { return 'error: ' + e; }
    })(),
    webglRenderer: (() => {
        try {
            const g = document.createElement('canvas').getContext('webgl');
            if (!g) return 'no webgl context';
            const ext = g.getExtension('WEBGL_debug_renderer_info');
            return g.getParameter(ext ? ext.UNMASKED_RENDERER_WEBGL : 37446);
        } catch(e) { return 'error: ' + e; }
    })(),
}, null, 2)"#;

const HE_SCRIPT: &str = r#"
navigator.userAgentData
    ? navigator.userAgentData.getHighEntropyValues([
        'platform','platformVersion','architecture','model','bitness','uaFullVersion'
      ]).then(v => JSON.stringify(v, null, 2))
    : Promise.resolve('"no userAgentData"')
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut headless = false;
    let mut no_sandbox = false;
    let mut virtual_display = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--headless"        => headless = true,
            "--no-sandbox"      => no_sandbox = true,
            "--virtual-display" => virtual_display = true,
            other => {
                eprintln!("Unknown: {other}");
                eprintln!("Usage: debug_fingerprint [--headless] [--no-sandbox] [--virtual-display]");
                return Ok(());
            }
        }
        i += 1;
    }

    let mut config = ChaserConfig::default()
        .with_headless(headless)
        .with_virtual_display(virtual_display);
    if no_sandbox {
        config = config.with_extra_args(vec!["--no-sandbox".to_string()]);
    }

    let mgr = BrowserManager::new(&config).await?;
    // Navigate to a real URL so addScriptToEvaluateOnNewDocument fires.
    // We use a neutral data: URL to avoid any external influence.
    let (page, _chaser) = mgr.new_page(None, "data:text/html,<html><body>fp</body></html>").await?;

    // Basic fingerprint
    let fp: Option<serde_json::Value> = page.evaluate(FP_SCRIPT).await?.into_value()?;
    if let Some(json_str) = fp {
        let parsed: serde_json::Value = serde_json::from_str(json_str.as_str().unwrap_or("{}"))?;
        println!("=== Fingerprint ===\n{}", serde_json::to_string_pretty(&parsed)?);
    }

    // High-entropy UA hints (returns a Promise, Page::evaluate resolves it)
    let he: Option<serde_json::Value> = page.evaluate(HE_SCRIPT).await?.into_value()?;
    if let Some(v) = he {
        let s = v.as_str().unwrap_or("{}");
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(parsed) => println!("\n=== UA High Entropy ===\n{}", serde_json::to_string_pretty(&parsed)?),
            Err(_) => println!("\n=== UA High Entropy ===\n{s}"),
        }
    }

    mgr.shutdown().await;
    Ok(())
}
