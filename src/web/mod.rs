mod api;
mod assets;

use crate::error::{ChronicleError, Result};
use crate::git::CliOps;

pub fn serve(git_ops: CliOps, port: u16, open_browser: bool) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let server = tiny_http::Server::http(&addr).map_err(|e| ChronicleError::Config {
        message: format!("failed to start web server on {addr}: {e}"),
        location: snafu::Location::default(),
    })?;

    println!("Chronicle web viewer: http://localhost:{port}");
    if open_browser {
        open::that(format!("http://localhost:{port}")).ok();
    }

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let result = if url.starts_with("/api/") {
            api::handle(&git_ops, &url)
        } else {
            Ok(assets::handle(&url))
        };

        match result {
            Ok(response) => {
                request.respond(response).ok();
            }
            Err(e) => {
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
