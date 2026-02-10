mod api;
mod assets;

use crate::error::{ChronicleError, Result};
use crate::git::CliOps;

pub fn serve(git_ops: CliOps, port: Option<u16>, open_browser: bool) -> Result<()> {
    let bind_port = port.unwrap_or(0);
    let listener = std::net::TcpListener::bind(("127.0.0.1", bind_port)).map_err(|e| {
        ChronicleError::Config {
            message: format!("failed to bind to port {bind_port}: {e}"),
            location: snafu::Location::default(),
        }
    })?;
    let actual_port = listener
        .local_addr()
        .map_err(|e| ChronicleError::Config {
            message: format!("failed to get local address: {e}"),
            location: snafu::Location::default(),
        })?
        .port();

    let server =
        tiny_http::Server::from_listener(listener, None).map_err(|e| ChronicleError::Config {
            message: format!("failed to start web server: {e}"),
            location: snafu::Location::default(),
        })?;

    let url = format!("http://localhost:{actual_port}");
    eprintln!("\n  Chronicle web viewer running at {url}\n  Press Ctrl+C to stop\n");
    if open_browser {
        open::that(&url).ok();
    }

    for request in server.incoming_requests() {
        let req_url = request.url().to_string();
        let result = if req_url.starts_with("/api/") {
            api::handle(&git_ops, &req_url)
        } else {
            Ok(assets::handle(&req_url))
        };

        match result {
            Ok(response) => {
                request.respond(response).ok();
            }
            Err(e) => {
                eprintln!("[chronicle-web] ERROR {req_url}: {e}");
                let body = serde_json::json!({ "error": e.to_string() }).to_string();
                let response = tiny_http::Response::from_string(body)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    )
                    .with_status_code(500);
                request.respond(response).ok();
            }
        }
    }

    Ok(())
}
