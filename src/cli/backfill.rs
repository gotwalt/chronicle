use crate::annotate::filter::{self, FilterDecision};
use crate::annotate::gather;
use crate::error::chronicle_error::{GitSnafu, IoSnafu};
use crate::error::Result;
use crate::git::CliOps;
use crate::git::GitOps;
use snafu::ResultExt;

pub fn run(limit: usize, dry_run: bool) -> Result<()> {
    // Find repo dir
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context(IoSnafu)?;

    if !output.status.success() {
        eprintln!("error: not in a git repository");
        std::process::exit(1);
    }

    let repo_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let ops = CliOps::new(std::path::PathBuf::from(&repo_dir));

    // Get recent commit SHAs
    let log_output = std::process::Command::new("git")
        .args(["log", "--format=%H", &format!("-{limit}")])
        .output()
        .context(IoSnafu)?;

    let shas: Vec<String> = String::from_utf8_lossy(&log_output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    eprintln!("Scanning last {} commits on HEAD...", shas.len());
    eprintln!();

    let mut annotate_count = 0;
    let mut skip_count = 0;
    let mut already_annotated = 0;

    for sha in &shas {
        // Check if already annotated
        let has_note = ops.note_exists(sha).context(GitSnafu)?;
        if has_note {
            already_annotated += 1;
            continue;
        }

        // Gather context for filtering
        let context = match gather::build_context(&ops, sha) {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!(
                    "  SKIP      {}  (error gathering context: {})",
                    &sha[..7],
                    e
                );
                skip_count += 1;
                continue;
            }
        };

        let short_sha = &sha[..7.min(sha.len())];
        let short_msg: String = context
            .commit_message
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(60)
            .collect();

        let decision = filter::pre_llm_filter(&context);
        match decision {
            FilterDecision::Annotate => {
                if dry_run {
                    let file_count = context.diffs.len();
                    let line_count: usize =
                        context.diffs.iter().map(|d| d.changed_line_count()).sum();
                    eprintln!(
                        "  ANNOTATE  {}  {} ({} files, {} lines)",
                        short_sha, short_msg, file_count, line_count
                    );
                } else {
                    eprint!(
                        "  [{}/{}] {}  {}...",
                        annotate_count + 1,
                        shas.len() - already_annotated,
                        short_sha,
                        short_msg
                    );
                    let provider = match crate::provider::discover_provider() {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("\nerror: {e}");
                            eprintln!("Run `git chronicle setup` or `git chronicle reconfigure` to select a provider.");
                            std::process::exit(1);
                        }
                    };
                    match crate::annotate::run(&ops, provider.as_ref(), sha, false) {
                        Ok(_) => eprintln!(" done"),
                        Err(e) => eprintln!(" error: {e}"),
                    }
                }
                annotate_count += 1;
            }
            FilterDecision::Skip(reason) => {
                if dry_run {
                    eprintln!("  SKIP      {}  {} ({})", short_sha, short_msg, reason);
                }
                skip_count += 1;
            }
            FilterDecision::Trivial(reason) => {
                if dry_run {
                    eprintln!("  SKIP      {}  {} ({})", short_sha, short_msg, reason);
                }
                skip_count += 1;
            }
        }
    }

    eprintln!();
    if dry_run {
        eprintln!(
            "Would annotate {} of {} commits ({} skipped, {} already annotated).",
            annotate_count,
            shas.len(),
            skip_count,
            already_annotated
        );
    } else if annotate_count > 0 {
        eprintln!(
            "Annotated {} commits ({} skipped, {} already annotated).",
            annotate_count, skip_count, already_annotated
        );
    } else {
        eprintln!("No commits to annotate.");
    }

    Ok(())
}
