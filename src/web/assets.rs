use std::io::Cursor;

#[derive(rust_embed::Embed)]
#[folder = "$WEB_DIST_DIR"]
struct WebAssets;

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

pub fn handle(url: &str) -> tiny_http::Response<Cursor<Vec<u8>>> {
    let path = url.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match WebAssets::get(path) {
        Some(file) => {
            let ct = content_type(path);
            tiny_http::Response::from_data(file.data.to_vec())
                .with_header(
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()).unwrap(),
                )
                .with_status_code(200)
        }
        None => {
            // SPA fallback: serve index.html for client-side routing
            match WebAssets::get("index.html") {
                Some(index) => tiny_http::Response::from_data(index.data.to_vec())
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"text/html; charset=utf-8"[..],
                        )
                        .unwrap(),
                    )
                    .with_status_code(200),
                None => tiny_http::Response::from_string("Not found").with_status_code(404),
            }
        }
    }
}
