use tokio::time::{timeout, Duration};
use reqwest::Client;

/// Scans the given subnet (e.g., "192.168.42.") for the Android app.
/// It checks IPs from 1 to 254 on port 8080 by calling GET /api/ping.
/// The Android app returns {"status":"ok","app":"usb_direct_share"}
pub async fn discover_phone(subnet: &str, port: u16) -> Option<String> {
    let client = Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .ok()?;

    let mut tasks = vec![];

    for i in 1..=254 {
        let ip = format!("{}{}", subnet, i);
        let client_clone = client.clone();

        let task = tokio::spawn(async move {
            let url = format!("http://{}:{}/api/ping", ip, port);
            if let Ok(Ok(response)) = timeout(Duration::from_millis(500), client_clone.get(&url).send()).await {
                if response.status().is_success() {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if json.get("app").and_then(|v| v.as_str()) == Some("usb_direct_share") {
                            return Some(ip);
                        }
                    }
                }
            }
            None
        });
        tasks.push(task);
    }

    for task in tasks {
        if let Ok(Some(ip)) = task.await {
            return Some(ip);
        }
    }

    None
}
