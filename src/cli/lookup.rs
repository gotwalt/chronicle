use crate::error::Result;
use crate::git::CliOps;

/// Run the `git chronicle lookup` command.
pub fn run(path: String, anchor: Option<String>, format: String, compact: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let output = crate::read::lookup::build_lookup(&git_ops, &path, anchor.as_deref()).map_err(
        |e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        },
    )?;

    match format.as_str() {
        "json" => {
            let json = if compact {
                let mut compact_out = serde_json::json!({
                    "contracts": output.contracts,
                    "dependencies": output.dependencies,
                    "decisions": output.decisions,
                    "recent_history": output.recent_history,
                    "open_follow_ups": output.open_follow_ups,
                    "staleness": output.staleness,
                });
                if let Some(ref k) = output.knowledge {
                    compact_out["knowledge"] = serde_json::to_value(k).unwrap_or_default();
                }
                serde_json::to_string_pretty(&compact_out)
            } else {
                serde_json::to_string_pretty(&output)
            }
            .map_err(|e| crate::error::ChronicleError::Json {
                source: e,
                location: snafu::Location::default(),
            })?;
            println!("{json}");
        }
        _ => {
            println!("Lookup for: {}", output.file);
            println!();

            if !output.contracts.is_empty() {
                println!("Contracts:");
                for c in &output.contracts {
                    let anchor_str = c
                        .anchor
                        .as_ref()
                        .map(|a| format!(":{}", a))
                        .unwrap_or_default();
                    println!(
                        "  [{}] {}{}: {}",
                        c.source, c.file, anchor_str, c.description
                    );
                }
                println!();
            }

            if !output.dependencies.is_empty() {
                println!("Dependencies:");
                for d in &output.dependencies {
                    let anchor_str = d
                        .anchor
                        .as_ref()
                        .map(|a| format!(":{}", a))
                        .unwrap_or_default();
                    println!(
                        "  {}{} -> {}:{} ({})",
                        d.file, anchor_str, d.target_file, d.target_anchor, d.assumption
                    );
                }
                println!();
            }

            if !output.decisions.is_empty() {
                println!("Decisions:");
                for d in &output.decisions {
                    println!("  [{}] {}: {}", d.stability, d.what, d.why);
                }
                println!();
            }

            if !output.recent_history.is_empty() {
                println!("Recent history:");
                for h in &output.recent_history {
                    println!("  {} {}: {}", &h.commit[..7.min(h.commit.len())], h.timestamp, h.intent);
                }
                println!();
            }

            if !output.open_follow_ups.is_empty() {
                println!("Open follow-ups:");
                for f in &output.open_follow_ups {
                    println!("  {} {}", &f.commit[..7.min(f.commit.len())], f.follow_up);
                }
                println!();
            }

            if let Some(ref knowledge) = output.knowledge {
                if !knowledge.conventions.is_empty() {
                    println!("Applicable conventions:");
                    for c in &knowledge.conventions {
                        println!("  [{}] {}", c.id, c.rule);
                    }
                    println!();
                }
                if !knowledge.boundaries.is_empty() {
                    println!("Module boundaries:");
                    for b in &knowledge.boundaries {
                        println!("  [{}] {}: {}", b.id, b.owns, b.boundary);
                    }
                    println!();
                }
                if !knowledge.anti_patterns.is_empty() {
                    println!("Anti-patterns:");
                    for a in &knowledge.anti_patterns {
                        println!("  [{}] Don't: {} -> {}", a.id, a.pattern, a.instead);
                    }
                    println!();
                }
            }

            if !output.staleness.is_empty() {
                let stale_entries: Vec<_> =
                    output.staleness.iter().filter(|s| s.stale).collect();
                if !stale_entries.is_empty() {
                    println!("Stale annotations:");
                    for s in &stale_entries {
                        println!(
                            "  {} ({} commits behind)",
                            &s.annotation_commit[..7.min(s.annotation_commit.len())],
                            s.commits_since
                        );
                    }
                    println!();
                }
            }

            if output.contracts.is_empty()
                && output.dependencies.is_empty()
                && output.decisions.is_empty()
                && output.recent_history.is_empty()
                && output.open_follow_ups.is_empty()
                && output.staleness.is_empty()
            {
                println!("  (no context found)");
            }
        }
    }

    Ok(())
}
