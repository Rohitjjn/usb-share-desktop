use axum::{
    body::Body,
    extract::{Query, Extension, Request},
    http::{StatusCode, Response, header},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::fs;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tower_http::services::fs::ServeFile;
use tower::ServiceExt;

use crate::server::ServerState;
use crate::path_utils::{resolve_and_verify_path, resolve_and_verify_path_for_creation};

#[derive(Serialize)]
#[derive(Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub last_modified: u64,
}

#[derive(Deserialize)]
pub struct PathQuery {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct ZipQuery {
    pub paths: String, // comma separated
}

pub async fn list_files(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Vec<FileInfo>>, StatusCode> {
    let req_path = query.path.unwrap_or_else(|| "/".to_string());

    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let resolved_path = resolve_and_verify_path(&root_path, &req_path).ok_or(StatusCode::NOT_FOUND)?;

    if !resolved_path.exists() || !resolved_path.is_dir() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut files = Vec::new();
    let mut entries = fs::read_dir(&resolved_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let meta = entry.metadata().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Construct relative path
        let rel_path = if req_path == "/" {
            format!("/{}", name)
        } else {
            let mut rp = req_path.clone();
            if !rp.ends_with('/') {
                rp.push('/');
            }
            rp.push_str(&name);
            rp
        };

        let last_modified = meta.modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        files.push(FileInfo {
            name,
            path: rel_path,
            is_dir: meta.is_dir(),
            size: meta.len(),
            last_modified,
        });
    }

    Ok(Json(files))
}

pub async fn download_file(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<PathQuery>,
    req: Request<Body>,
) -> Response<Body> {
    let req_path = match query.path {
        Some(p) => p,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let resolved_path = match resolve_and_verify_path(&root_path, &req_path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    if !resolved_path.exists() || resolved_path.is_dir() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let serve_file = ServeFile::new(resolved_path);
    match serve_file.oneshot(req).await {
        Ok(res) => res.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn upload_file(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<PathQuery>,
    req: Request<Body>,
) -> Response<Body> {
    let req_path = match query.path {
        Some(p) => p,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let resolved_path = match resolve_and_verify_path_for_creation(&root_path, &req_path) {
        Some(p) => p,
        None => return StatusCode::FORBIDDEN.into_response(),
    };

    let mut file = match tokio::fs::File::create(&resolved_path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut body_stream = req.into_body().into_data_stream();
    while let Some(chunk) = body_stream.next().await {
        if let Ok(bytes) = chunk {
            if file.write_all(&bytes).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    StatusCode::CREATED.into_response()
}

pub async fn mkdir(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<PathQuery>,
) -> Response<Body> {
    let req_path = match query.path {
        Some(p) => p,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let resolved_path = match resolve_and_verify_path_for_creation(&root_path, &req_path) {
        Some(p) => p,
        None => return StatusCode::FORBIDDEN.into_response(),
    };

    match fs::create_dir(&resolved_path).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

#[derive(Deserialize)]
pub struct RenameQuery {
    pub path: String,
    pub new_path: String,
}

pub async fn rename_file(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<RenameQuery>,
) -> Response<Body> {
    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let src_path = match resolve_and_verify_path(&root_path, &query.path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let dest_path = match resolve_and_verify_path_for_creation(&root_path, &query.new_path) {
        Some(p) => p,
        None => return StatusCode::FORBIDDEN.into_response(),
    };

    match fs::rename(&src_path, &dest_path).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn delete_file(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<PathQuery>,
) -> Response<Body> {
    let req_path = match query.path {
        Some(p) => p,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let resolved_path = match resolve_and_verify_path(&root_path, &req_path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let result = if resolved_path.is_dir() {
        fs::remove_dir_all(&resolved_path).await
    } else {
        fs::remove_file(&resolved_path).await
    };

    match result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// To stream zip we use temp file, then ServeFile, because zip crate requires Seek,
// which our ChannelWriter cannot easily provide if we want to stream directly.
pub async fn zip_files(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    Query(query): Query<ZipQuery>,
    req: Request<Body>,
) -> Response<Body> {
    let paths: Vec<String> = query.paths.split(',').map(|s| s.to_string()).collect();
    if paths.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    let mut valid_paths = Vec::new();
    for p in paths {
        if let Some(rp) = resolve_and_verify_path(&root_path, &p) {
            valid_paths.push((p, rp));
        }
    }

    if valid_paths.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let temp_zip_path = std::env::temp_dir().join(format!("{}.zip", uuid::Uuid::new_v4()));
    let temp_zip_path_clone = temp_zip_path.clone();

    // Spawn a blocking task to write to the zip archive
    let result = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::create(&temp_zip_path_clone)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (req_path, fs_path) in valid_paths {
            if fs_path.is_file() {
                if let Ok(mut f) = std::fs::File::open(&fs_path) {
                    if zip.start_file(req_path.trim_start_matches('/'), options).is_ok() {
                        let _ = std::io::copy(&mut f, &mut zip);
                    }
                }
            } else if fs_path.is_dir() {
                // Simplified: just add empty dir or walk. For a full implementation, use WalkDir
                let _ = zip.add_directory(req_path.trim_start_matches('/'), options);
            }
        }
        zip.finish()?;
        Ok::<(), std::io::Error>(())
    }).await;

    match result {
        Ok(Ok(_)) => {
            let serve_file = ServeFile::new(&temp_zip_path);

            // Clean up the temp file after a short delay so it can be served
            let temp_cleanup_path = temp_zip_path.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let _ = tokio::fs::remove_file(temp_cleanup_path).await;
            });

            let mut res = match serve_file.oneshot(req).await {
                Ok(res) => res.into_response(),
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };

            res.headers_mut().insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
            res.headers_mut().insert(header::CONTENT_DISPOSITION, "attachment; filename=\"download.zip\"".parse().unwrap());

            res
        },
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
