use crate::error::Result;
use crate::git::{CliOps, GitOps};

#[derive(serde::Serialize)]
pub struct StatusOutput {
    pub total_annotations: usize,
    pub recent_commits: usize,
    pub recent_annotated: usize,
    pub coverage_pct: f64,
    pub unannotated_commits: Vec<String>,
}

/// Build status data from a GitOps instance. Separated from `run()` so the web
/// API can call it directly without printing.
pub fn build_status(git_ops: &dyn GitOps) -> Result<StatusOutput> {
    let annotated =
        git_ops
            .list_annotated_commits(10000)
            .map_err(|e| crate::error::ChronicleError::Git {
                source: e,
                location: snafu::Location::default(),
            })?;
    let annotated_set: std::collections::HashSet<_> = annotated.iter().collect();

    let recent_shas = get_recent_commits_dyn(git_ops, 20)?;
    let recent_count = recent_shas.len();

    let mut annotated_count = 0;
    let mut unannotated = Vec::new();
    for sha in &recent_shas {
        if annotated_set.contains(sha) {
            annotated_count += 1;
        } else {
            unannotated.push(sha.clone());
        }
    }

    let coverage = if recent_count > 0 {
        (annotated_count as f64 / recent_count as f64) * 100.0
    } else {
        0.0
    };

    Ok(StatusOutput {
        total_annotations: annotated.len(),
        recent_commits: recent_count,
        recent_annotated: annotated_count,
        coverage_pct: (coverage * 10.0).round() / 10.0,
        unannotated_commits: unannotated,
    })
}

pub fn run(format: String) -> Result<()> {
    let _ = format; // reserved for future pretty-print support
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let output = build_status(&git_ops)?;

    let json =
        serde_json::to_string_pretty(&output).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;
    println!("{json}");

    Ok(())
}

/// Get recent commits using the git log command. Works with any GitOps via
/// resolve_ref to find HEAD then walking back, but for simplicity we shell out.
fn get_recent_commits_dyn(git_ops: &dyn GitOps, count: usize) -> Result<Vec<String>> {
    // We need the repo_dir from CliOps. Since this is only called from CLI
    // contexts where we have CliOps, use the resolve_ref approach to get HEAD
    // then use git log. For the generic case, we need to shell out.
    // Since GitOps doesn't expose a "log recent N" method, we'll get the
    // repo dir from the current directory (same as how run() works).
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;

    // Verify the git ops is working by resolving HEAD
    let _head = git_ops
        .resolve_ref("HEAD")
        .map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    let output = std::process::Command::new("git")
        .args(["log", "--format=%H", &format!("-{count}")])
        .current_dir(&repo_dir)
        .output()
        .map_err(|e| crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect())
}
