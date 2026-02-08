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

pub fn run(format: String) -> Result<()> {
    let _ = format; // reserved for future pretty-print support
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    // Get all annotated commits
    let annotated =
        git_ops
            .list_annotated_commits(10000)
            .map_err(|e| crate::error::ChronicleError::Git {
                source: e,
                location: snafu::Location::default(),
            })?;
    let annotated_set: std::collections::HashSet<_> = annotated.iter().collect();

    // Get recent commits (last 20 SHAs)
    let recent_shas = get_recent_commits(&git_ops, 20)?;
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

    let output = StatusOutput {
        total_annotations: annotated.len(),
        recent_commits: recent_count,
        recent_annotated: annotated_count,
        coverage_pct: (coverage * 10.0).round() / 10.0,
        unannotated_commits: unannotated,
    };

    let json =
        serde_json::to_string_pretty(&output).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;
    println!("{json}");

    Ok(())
}

fn get_recent_commits(git_ops: &CliOps, count: usize) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["log", "--format=%H", &format!("-{count}")])
        .current_dir(&git_ops.repo_dir)
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
