use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{any, get, post},
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

use crate::webdav::webdav_handler;
use crate::rest::*;

#[derive(Clone)]
pub struct ServerState {
    pub root_path: PathBuf,
    pub credentials: Option<(String, String)>, // (username, password)
}

pub struct AppState {
    pub state: Arc<Mutex<ServerState>>,
}

pub async fn basic_auth(
    req: Request<Body>,
    next: Next,
) -> Result<axum::response::Response, StatusCode> {
    // Exclude /api/ping from auth
    if req.uri().path() == "/api/ping" {
        return Ok(next.run(req).await);
    }

    let state = req
        .extensions()
        .get::<Arc<Mutex<ServerState>>>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let credentials = {
        let state = state.lock().unwrap();
        state.credentials.clone()
    };

    if let Some((expected_user, expected_pass)) = credentials {
        if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(b64_creds) = auth_str.strip_prefix("Basic ") {

                    if let Ok(decoded) = general_purpose::STANDARD.decode(b64_creds) {
                        if let Ok(decoded_str) = String::from_utf8(decoded) {
                            let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
                            if parts.len() == 2 && parts[0] == expected_user && parts[1] == expected_pass {
                                return Ok(next.run(req).await);
                            }
                        }
                    }
                }
            }
        }

        // Return 401 with WWW-Authenticate header
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"USB Direct Share\"".parse().unwrap(),
        );
        return Ok(response);
    }

    Ok(next.run(req).await)
}

pub async fn build_app(state: Arc<Mutex<ServerState>>) -> Router {
    Router::new()
        .route("/api/ping", get(|| async { axum::Json(serde_json::json!({"status":"ok","app":"usb_direct_share"})) }))
        .route("/api/list", get(list_files))
        .route("/api/download", get(download_file))
        .route("/api/upload", post(upload_file))
        .route("/api/mkdir", post(mkdir))
        .route("/api/rename", post(rename_file))
        .route("/api/delete", post(delete_file))
        .route("/api/zip", get(zip_files))
        .fallback(any(webdav_handler))
        .layer(middleware::from_fn(basic_auth))
        .layer(axum::Extension(state))
}

pub async fn start_server(
    addr: SocketAddr,
    state: Arc<Mutex<ServerState>>,
) -> Result<(), std::io::Error> {
    let app = build_app(state).await;
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_ping_endpoint_no_auth() {
        let state = Arc::new(Mutex::new(ServerState {
            root_path: PathBuf::from("."),
            credentials: Some(("admin".to_string(), "pass".to_string())),
        }));

        let app = build_app(state).await;

        let response = app
            .oneshot(Request::builder().uri("/api/ping").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_required_for_other_endpoints() {
        let state = Arc::new(Mutex::new(ServerState {
            root_path: PathBuf::from("."),
            credentials: Some(("admin".to_string(), "pass".to_string())),
        }));

        let app = build_app(state).await;

        let response = app
            .oneshot(Request::builder().uri("/api/list?path=/").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
    }
}
