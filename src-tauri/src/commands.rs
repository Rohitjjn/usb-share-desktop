use std::sync::{Arc, Mutex};
use std::net::{SocketAddr, IpAddr};
use std::path::PathBuf;
use tauri::State;
use network_interface::{NetworkInterface, NetworkInterfaceConfig, Addr};
use rand::{distributions::Alphanumeric, Rng};

use crate::server::AppState;
use crate::discovery::discover_phone;

// Using a struct to hold the abort handle so we can stop the server
pub struct ServerHandle {
    pub abort_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[derive(serde::Serialize)]
pub struct ServerStatus {
    pub is_running: bool,
    pub ip: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

#[tauri::command]
pub async fn start_local_server(
    root_path_str: String,
    state: State<'_, AppState>,
    handle: State<'_, ServerHandle>,
) -> Result<ServerStatus, String> {
    let root_path = PathBuf::from(root_path_str);
    if !root_path.exists() {
        return Err("Selected folder does not exist".to_string());
    }

    let username = "usbshare".to_string();
    let password: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();

    // Update state
    {
        let mut st = state.state.lock().unwrap();
        st.root_path = root_path;
        st.credentials = Some((username.clone(), password.clone()));
    }

    // Stop existing if running
    stop_local_server(handle.clone()).await?;

    let ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let port = 8080;

    // Bind address
    let bind_ip: IpAddr = ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED.into());
    let addr = SocketAddr::new(bind_ip, port);

    let state_clone = Arc::clone(&state.state);

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    *handle.abort_tx.lock().unwrap() = Some(tx);

    tokio::spawn(async move {
        // We use axum::serve with graceful shutdown
        if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
            let app = crate::server::build_app(state_clone).await;
            let _ = axum::serve(listener, app).with_graceful_shutdown(async {
                rx.await.ok();
            }).await;
        }
    });

    Ok(ServerStatus {
        is_running: true,
        ip,
        port,
        username,
        password,
    })
}

#[tauri::command]
pub async fn stop_local_server(handle: State<'_, ServerHandle>) -> Result<(), String> {
    let mut abort_tx = handle.abort_tx.lock().unwrap();
    if let Some(tx) = abort_tx.take() {
        let _ = tx.send(());
    }
    Ok(())
}

#[tauri::command]
pub fn get_local_ip() -> Option<String> {
    let network_interfaces = NetworkInterface::show().ok()?;
    for itf in network_interfaces.iter() {
        // Try to find a non-loopback IPv4 address
        // Note: For USB tethering, the interface name might be like 'rndis' or 'en' or 'eth'
        for addr in itf.addr.iter() {
            if let Addr::V4(ipv4) = addr {
                if ipv4.ip.to_string() != "127.0.0.1" {
                    return Some(ipv4.ip.to_string());
                }
            }
        }
    }
    None
}

#[tauri::command]
pub async fn discover_phone_cmd() -> Result<String, String> {
    let local_ip = get_local_ip().unwrap_or_else(|| "192.168.42.2".to_string());
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() != 4 {
        return Err("Invalid local IP format".to_string());
    }

    let subnet = format!("{}.{}.{}.", parts[0], parts[1], parts[2]);
    let port = 8080;

    if let Some(ip) = discover_phone(&subnet, port).await {
        Ok(ip)
    } else {
        Err("Phone not found on the tethering network. Ensure USB tethering is active and the Android app server is running.".to_string())
    }
}
