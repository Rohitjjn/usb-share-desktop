use reqwest::{Client, StatusCode};
use base64::{engine::general_purpose, Engine as _};
use std::time::Duration;
use crate::rest::FileInfo;

pub struct PhoneClient {
    client: Client,
    base_url: String,
    auth_header: String,
}

impl PhoneClient {
    pub fn new(ip: &str, port: u16, user: &str, pass: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let base_url = format!("http://{}:{}", ip, port);

        let auth = format!("{}:{}", user, pass);
        let b64_auth = general_purpose::STANDARD.encode(auth);
        let auth_header = format!("Basic {}", b64_auth);

        Self {
            client,
            base_url,
            auth_header,
        }
    }

    pub async fn list_files(&self, path: &str) -> Result<Vec<FileInfo>, String> {
        let url = format!("{}/api/list?path={}", self.base_url, path);
        let res = self.client.get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            return Err(format!("Server returned error: {}", res.status()));
        }

        let files = res.json::<Vec<FileInfo>>().await.map_err(|e| e.to_string())?;
        Ok(files)
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        let url = format!("{}/api/mkdir?path={}", self.base_url, path);
        let res = self.client.post(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() && res.status() != StatusCode::CONFLICT {
            return Err(format!("Failed to create folder: {}", res.status()));
        }
        Ok(())
    }

    pub async fn rename(&self, path: &str, new_path: &str) -> Result<(), String> {
        let url = format!("{}/api/rename?path={}&new_path={}", self.base_url, path, new_path);
        let res = self.client.post(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            return Err(format!("Failed to rename: {}", res.status()));
        }
        Ok(())
    }

    pub async fn delete(&self, path: &str) -> Result<(), String> {
        let url = format!("{}/api/delete?path={}", self.base_url, path);
        let res = self.client.post(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() && res.status() != StatusCode::NOT_FOUND {
            return Err(format!("Failed to delete: {}", res.status()));
        }
        Ok(())
    }

    // For larger files, we might want streaming download/upload via frontend,
    // but we can provide simple rust wrappers if needed by Tauri commands.
    // For now, returning the base_url and auth_header so frontend can fetch directly is better
    // because it natively handles streaming, progress events, and saving via native dialogs.

    pub fn get_connection_info(&self) -> (String, String) {
        (self.base_url.clone(), self.auth_header.clone())
    }
}
