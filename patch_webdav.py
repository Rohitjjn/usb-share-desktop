import sys

def patch():
    with open('src-tauri/src/webdav.rs', 'r') as f:
        content = f.read()

    old = """async fn handle_options() -> Response<Body> {
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Allow", "OPTIONS, PROPFIND, GET, PUT, MKCOL, DELETE, MOVE, LOCK, UNLOCK")
        .header("DAV", "1, 2")
        .header("MS-Author-Via", "DAV")
        .body(Body::empty())
        .unwrap();
    response
}"""
    new = """async fn handle_options() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Allow", "OPTIONS, PROPFIND, GET, PUT, MKCOL, DELETE, MOVE, LOCK, UNLOCK")
        .header("DAV", "1, 2")
        .header("MS-Author-Via", "DAV")
        .body(Body::empty())
        .unwrap()
}"""
    content = content.replace(old, new)
    
    with open('src-tauri/src/webdav.rs', 'w') as f:
        f.write(content)

patch()
