use axum::{
    body::Body,
    extract::{Request, Extension},
    http::{header, Method, StatusCode, Response},
    response::IntoResponse,
};
use std::sync::{Arc, Mutex};
use std::path::Path;
use tokio::fs;
use quick_xml::events::{Event, BytesText, BytesStart, BytesEnd, BytesDecl};
use quick_xml::Writer;
use quick_xml::escape::escape;
use std::io::Cursor;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use percent_encoding::percent_decode_str;

use crate::server::ServerState;
use crate::path_utils::{resolve_and_verify_path, resolve_and_verify_path_for_creation};

pub async fn webdav_handler(
    Extension(state): Extension<Arc<Mutex<ServerState>>>,
    req: Request<Body>,
) -> Response<Body> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let root_path = {
        let st = state.lock().unwrap();
        st.root_path.clone()
    };

    // Decode the path
    let decoded_path = percent_decode_str(&path).decode_utf8_lossy().to_string();

    match method {
        Method::OPTIONS => handle_options().await,
        m if m == Method::from_bytes(b"PROPFIND").unwrap() => handle_propfind(&root_path, &decoded_path, req).await,
        Method::GET => handle_get(&root_path, &decoded_path, req).await,
        Method::PUT => handle_put(&root_path, &decoded_path, req).await,
        m if m == Method::from_bytes(b"MKCOL").unwrap() => handle_mkcol(&root_path, &decoded_path).await,
        Method::DELETE => handle_delete(&root_path, &decoded_path).await,
        m if m == Method::from_bytes(b"MOVE").unwrap() => handle_move(&root_path, &decoded_path, req).await,
        m if m == Method::from_bytes(b"LOCK").unwrap() => handle_lock().await,
        m if m == Method::from_bytes(b"UNLOCK").unwrap() => handle_unlock().await,
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

async fn handle_options() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Allow", "OPTIONS, PROPFIND, GET, PUT, MKCOL, DELETE, MOVE, LOCK, UNLOCK")
        .header("DAV", "1, 2")
        .header("MS-Author-Via", "DAV")
        .body(Body::empty())
        .unwrap()
}

async fn handle_propfind(root_path: &Path, request_path: &str, req: Request<Body>) -> Response<Body> {
    let depth = req.headers().get("Depth").and_then(|h| h.to_str().ok()).unwrap_or("1");
    let resolved_path = match resolve_and_verify_path(root_path, request_path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    if !resolved_path.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None))).unwrap();

    let mut multistatus = BytesStart::new("D:multistatus");
    multistatus.push_attribute(("xmlns:D", "DAV:"));
    writer.write_event(Event::Start(multistatus)).unwrap();

    let meta = match fs::metadata(&resolved_path).await {
        Ok(m) => m,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    write_response(&mut writer, request_path, &meta);

    if meta.is_dir() && depth == "1" {
        if let Ok(mut entries) = fs::read_dir(&resolved_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(meta) = entry.metadata().await {
                    let mut child_req_path = request_path.to_string();
                    if !child_req_path.ends_with('/') {
                        child_req_path.push('/');
                    }
                    child_req_path.push_str(&entry.file_name().to_string_lossy());
                    write_response(&mut writer, &child_req_path, &meta);
                }
            }
        }
    }

    writer.write_event(Event::End(BytesEnd::new("D:multistatus"))).unwrap();

    let xml = String::from_utf8(writer.into_inner().into_inner()).unwrap();
    Response::builder()
        .status(StatusCode::MULTI_STATUS)
        .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
        .body(Body::from(xml))
        .unwrap()
}

fn write_response(writer: &mut Writer<Cursor<Vec<u8>>>, request_path: &str, meta: &std::fs::Metadata) {
    writer.write_event(Event::Start(BytesStart::new("D:response"))).unwrap();

    // href
    writer.write_event(Event::Start(BytesStart::new("D:href"))).unwrap();
    let mut href = String::from(request_path);
    if meta.is_dir() && !href.ends_with('/') {
        href.push('/');
    }
    writer.write_event(Event::Text(BytesText::new(&escape(&href)))).unwrap();
    writer.write_event(Event::End(BytesEnd::new("D:href"))).unwrap();

    // propstat
    writer.write_event(Event::Start(BytesStart::new("D:propstat"))).unwrap();
    writer.write_event(Event::Start(BytesStart::new("D:prop"))).unwrap();

    // resourcetype
    writer.write_event(Event::Start(BytesStart::new("D:resourcetype"))).unwrap();
    if meta.is_dir() {
        writer.write_event(Event::Empty(BytesStart::new("D:collection"))).unwrap();
    }
    writer.write_event(Event::End(BytesEnd::new("D:resourcetype"))).unwrap();

    // getcontentlength
    if !meta.is_dir() {
        writer.write_event(Event::Start(BytesStart::new("D:getcontentlength"))).unwrap();
        writer.write_event(Event::Text(BytesText::new(&meta.len().to_string()))).unwrap();
        writer.write_event(Event::End(BytesEnd::new("D:getcontentlength"))).unwrap();
    }

    // getlastmodified
    if let Ok(modified) = meta.modified() {
        let datetime: DateTime<Utc> = modified.into();
        writer.write_event(Event::Start(BytesStart::new("D:getlastmodified"))).unwrap();
        writer.write_event(Event::Text(BytesText::new(&datetime.to_rfc2822()))).unwrap();
        writer.write_event(Event::End(BytesEnd::new("D:getlastmodified"))).unwrap();
    }

    writer.write_event(Event::End(BytesEnd::new("D:prop"))).unwrap();

    writer.write_event(Event::Start(BytesStart::new("D:status"))).unwrap();
    writer.write_event(Event::Text(BytesText::new("HTTP/1.1 200 OK"))).unwrap();
    writer.write_event(Event::End(BytesEnd::new("D:status"))).unwrap();

    writer.write_event(Event::End(BytesEnd::new("D:propstat"))).unwrap();

    writer.write_event(Event::End(BytesEnd::new("D:response"))).unwrap();
}

async fn handle_get(root_path: &Path, request_path: &str, req: Request<Body>) -> Response<Body> {
    let resolved_path = match resolve_and_verify_path(root_path, request_path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    if !resolved_path.exists() || resolved_path.is_dir() {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Using tower_http::services::fs::ServeFile for range request support
    use tower_http::services::fs::ServeFile;
    use tower::ServiceExt;

    let serve_file = ServeFile::new(resolved_path);
    match serve_file.oneshot(req).await {
        Ok(res) => res.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn handle_put(root_path: &Path, request_path: &str, req: Request<Body>) -> Response<Body> {
    let resolved_path = match resolve_and_verify_path_for_creation(root_path, request_path) {
        Some(p) => p,
        None => return StatusCode::FORBIDDEN.into_response(),
    };

    use tokio::io::AsyncWriteExt;
    use futures_util::StreamExt;
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

async fn handle_mkcol(root_path: &Path, request_path: &str) -> Response<Body> {
    let resolved_path = match resolve_and_verify_path_for_creation(root_path, request_path) {
        Some(p) => p,
        None => return StatusCode::FORBIDDEN.into_response(),
    };

    if resolved_path.exists() {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    match fs::create_dir(&resolved_path).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

async fn handle_delete(root_path: &Path, request_path: &str) -> Response<Body> {
    let resolved_path = match resolve_and_verify_path(root_path, request_path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    if !resolved_path.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

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

async fn handle_move(root_path: &Path, request_path: &str, req: Request<Body>) -> Response<Body> {
    let src_path = match resolve_and_verify_path(root_path, request_path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let destination = match req.headers().get("Destination") {
        Some(d) => match d.to_str() {
            Ok(s) => s,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        },
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Extract path from destination URL
    let dest_url = match url::Url::parse(destination) {
        Ok(u) => u,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let dest_req_path = percent_decode_str(dest_url.path()).decode_utf8_lossy().to_string();

    let dest_path = match resolve_and_verify_path_for_creation(root_path, &dest_req_path) {
        Some(p) => p,
        None => return StatusCode::FORBIDDEN.into_response(),
    };

    let overwrite = req.headers().get("Overwrite").and_then(|h| h.to_str().ok()).unwrap_or("T") == "T";

    if dest_path.exists() && !overwrite {
        return StatusCode::PRECONDITION_FAILED.into_response();
    }

    match fs::rename(&src_path, &dest_path).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn handle_lock() -> Response<Body> {
    let token = format!("urn:uuid:{}", Uuid::new_v4());
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:locktype><D:write/></D:locktype>
      <D:lockscope><D:exclusive/></D:lockscope>
      <D:depth>Infinity</D:depth>
      <D:timeout>Second-604800</D:timeout>
      <D:locktoken>
        <D:href>{}</D:href>
      </D:locktoken>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#,
        token
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
        .header("Lock-Token", format!("<{}>", token))
        .body(Body::from(xml))
        .unwrap()
}

async fn handle_unlock() -> Response<Body> {
    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
     // For req.into_body().collect() if needed

    #[tokio::test]
    async fn test_handle_options() {
        let res = handle_options().await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("Allow").unwrap(), "OPTIONS, PROPFIND, GET, PUT, MKCOL, DELETE, MOVE, LOCK, UNLOCK");
    }

    #[tokio::test]
    async fn test_handle_lock_unlock() {
        let lock_res = handle_lock().await;
        assert_eq!(lock_res.status(), StatusCode::OK);
        assert!(lock_res.headers().get("Lock-Token").is_some());

        let unlock_res = handle_unlock().await;
        assert_eq!(unlock_res.status(), StatusCode::NO_CONTENT);
    }
}
