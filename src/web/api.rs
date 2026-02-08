use std::io::Cursor;
use std::path::Path;

use crate::cli::status::build_status;
use crate::error::{ChronicleError, GitError};
use crate::git::{CliOps, GitOps};
use crate::knowledge;
use crate::read::{decisions, history, lookup, summary};

type Response = tiny_http::Response<Cursor<Vec<u8>>>;

pub fn handle(git_ops: &CliOps, url: &str) -> crate::error::Result<Response> {
    // Strip query string for route matching, keep it for param parsing
    let (path, query) = url.split_once('?').unwrap_or((url, ""));

    match path {
        "/api/status" => handle_status(git_ops),
        "/api/tree" => handle_tree(git_ops),
        "/api/decisions" => handle_decisions(git_ops, query),
        "/api/knowledge" => handle_knowledge(git_ops),
        _ if path.starts_with("/api/file-view/") => {
            let file_path = &path["/api/file-view/".len()..];
            handle_file_view(git_ops, file_path)
        }
        _ if path.starts_with("/api/file/") => {
            let file_path = &path["/api/file/".len()..];
            handle_file(git_ops, file_path)
        }
        _ if path.starts_with("/api/lookup/") => {
            let file_path = &path["/api/lookup/".len()..];
            handle_lookup(git_ops, file_path, query)
        }
        _ if path.starts_with("/api/summary/") => {
            let file_path = &path["/api/summary/".len()..];
            handle_summary(git_ops, file_path)
        }
        _ if path.starts_with("/api/history/") => {
            let file_path = &path["/api/history/".len()..];
            handle_history(git_ops, file_path, query)
        }
        _ => Ok(json_response(404, &serde_json::json!({"error": "not found"}))),
    }
}

fn handle_status(git_ops: &CliOps) -> crate::error::Result<Response> {
    let output = build_status(git_ops)?;
    Ok(json_response(200, &output))
}

fn handle_tree(git_ops: &CliOps) -> crate::error::Result<Response> {
    let files = list_files(git_ops)?;
    let annotated = git_ops
        .list_annotated_commits(10000)
        .map_err(|e| ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    // Build a map of file -> annotation count by scanning all annotations
    let mut annotation_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for sha in &annotated {
        if let Ok(Some(note)) = git_ops.note_read(sha) {
            if let Ok(ann) = crate::schema::parse_annotation(&note) {
                for f in &ann.narrative.files_changed {
                    *annotation_counts.entry(f.clone()).or_insert(0) += 1;
                }
                for m in &ann.markers {
                    annotation_counts.entry(m.file.clone()).or_insert(0);
                }
            }
        }
    }

    let tree_files: Vec<serde_json::Value> = files
        .iter()
        .map(|f| {
            serde_json::json!({
                "path": f,
                "annotation_count": annotation_counts.get(f.as_str()).unwrap_or(&0),
            })
        })
        .collect();

    Ok(json_response(200, &serde_json::json!({ "files": tree_files })))
}

fn handle_file(git_ops: &CliOps, file_path: &str) -> crate::error::Result<Response> {
    let content = git_ops
        .file_at_commit(Path::new(file_path), "HEAD")
        .map_err(|e| ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;
    let language = detect_language(file_path);

    Ok(json_response(
        200,
        &serde_json::json!({
            "path": file_path,
            "content": content,
            "language": language,
        }),
    ))
}

fn handle_file_view(git_ops: &CliOps, file_path: &str) -> crate::error::Result<Response> {
    let content = git_ops
        .file_at_commit(Path::new(file_path), "HEAD")
        .map_err(|e| ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;
    let language = detect_language(file_path);

    let lookup_result = lookup::build_lookup(git_ops, file_path, None).map_err(|e| {
        ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    let summary_result = summary::build_summary(
        git_ops,
        &summary::SummaryQuery {
            file: file_path.to_string(),
            anchor: None,
        },
    )
    .map_err(|e| ChronicleError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;

    Ok(json_response(
        200,
        &serde_json::json!({
            "path": file_path,
            "content": content,
            "language": language,
            "lookup": lookup_result,
            "summary": summary_result,
        }),
    ))
}

fn handle_lookup(
    git_ops: &CliOps,
    file_path: &str,
    query: &str,
) -> crate::error::Result<Response> {
    let anchor = parse_query_param(query, "anchor");
    let result =
        lookup::build_lookup(git_ops, file_path, anchor.as_deref()).map_err(|e| {
            ChronicleError::Git {
                source: e,
                location: snafu::Location::default(),
            }
        })?;
    Ok(json_response(200, &result))
}

fn handle_summary(git_ops: &CliOps, file_path: &str) -> crate::error::Result<Response> {
    let result = summary::build_summary(
        git_ops,
        &summary::SummaryQuery {
            file: file_path.to_string(),
            anchor: None,
        },
    )
    .map_err(|e| ChronicleError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;
    Ok(json_response(200, &result))
}

fn handle_history(
    git_ops: &CliOps,
    file_path: &str,
    query: &str,
) -> crate::error::Result<Response> {
    let anchor = parse_query_param(query, "anchor");
    let limit = parse_query_param(query, "limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10u32);

    let result = history::build_timeline(
        git_ops,
        &history::HistoryQuery {
            file: file_path.to_string(),
            anchor,
            limit,
        },
    )
    .map_err(|e| ChronicleError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;
    Ok(json_response(200, &result))
}

fn handle_decisions(git_ops: &CliOps, query: &str) -> crate::error::Result<Response> {
    let file = parse_query_param(query, "path");
    let result = decisions::query_decisions(git_ops, &decisions::DecisionsQuery { file }).map_err(
        |e| ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        },
    )?;
    Ok(json_response(200, &result))
}

fn handle_knowledge(git_ops: &CliOps) -> crate::error::Result<Response> {
    let store = knowledge::read_store(git_ops).map_err(|e| ChronicleError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;
    Ok(json_response(200, &store))
}

// --- Helpers ---

fn json_response(status: u16, body: &impl serde::Serialize) -> Response {
    let json = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    tiny_http::Response::from_string(json)
        .with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        )
        .with_header(
            tiny_http::Header::from_bytes(
                &b"Access-Control-Allow-Origin"[..],
                &b"*"[..],
            )
            .unwrap(),
        )
        .with_status_code(status)
}

fn parse_query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == key {
            Some(urldecode(v))
        } else {
            None
        }
    })
}

fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        match b {
            b'%' => {
                let hi = chars.next().and_then(hex_val);
                let lo = chars.next().and_then(hex_val);
                if let (Some(h), Some(l)) = (hi, lo) {
                    result.push((h << 4 | l) as char);
                }
            }
            b'+' => result.push(' '),
            _ => result.push(b as char),
        }
    }
    result
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn list_files(git_ops: &CliOps) -> Result<Vec<String>, ChronicleError> {
    let output = std::process::Command::new("git")
        .args(["ls-tree", "-r", "--name-only", "HEAD"])
        .current_dir(&git_ops.repo_dir)
        .output()
        .map_err(|e| ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

    if !output.status.success() {
        return Err(ChronicleError::Git {
            source: GitError::CommandFailed {
                message: "git ls-tree failed".to_string(),
                location: snafu::Location::default(),
            },
            location: snafu::Location::default(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect())
}

fn detect_language(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("rs") => "rust",
        Some("ts") | Some("tsx") => "typescript",
        Some("js") | Some("jsx") => "javascript",
        Some("py") => "python",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") => "cpp",
        Some("rb") => "ruby",
        Some("sh") | Some("bash") => "bash",
        Some("yml") | Some("yaml") => "yaml",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        Some("html") => "html",
        Some("css") => "css",
        Some("sql") => "sql",
        Some("zig") => "zig",
        Some("lua") => "lua",
        Some("swift") => "swift",
        Some("kt") | Some("kts") => "kotlin",
        Some("cs") => "csharp",
        _ => "text",
    }
}
