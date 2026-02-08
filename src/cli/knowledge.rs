use crate::error::Result;
use crate::git::CliOps;
use crate::knowledge;
use crate::schema::knowledge::{AntiPattern, Convention, KnowledgeStore, ModuleBoundary};
use crate::schema::v2::Stability;

/// Run `git chronicle knowledge list`.
pub fn run_list(json: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let store = knowledge::read_store(&git_ops).map_err(|e| crate::error::ChronicleError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;

    if json {
        let output = serde_json::to_string_pretty(&store).map_err(|e| {
            crate::error::ChronicleError::Json {
                source: e,
                location: snafu::Location::default(),
            }
        })?;
        println!("{output}");
    } else {
        if store.is_empty() {
            println!("No knowledge entries.");
            return Ok(());
        }
        if !store.conventions.is_empty() {
            println!("Conventions:");
            for c in &store.conventions {
                println!(
                    "  [{}] scope={} stability={:?}: {}",
                    c.id, c.scope, c.stability, c.rule
                );
            }
            println!();
        }
        if !store.boundaries.is_empty() {
            println!("Module boundaries:");
            for b in &store.boundaries {
                println!(
                    "  [{}] {} — owns: {}, boundary: {}",
                    b.id, b.module, b.owns, b.boundary
                );
            }
            println!();
        }
        if !store.anti_patterns.is_empty() {
            println!("Anti-patterns:");
            for a in &store.anti_patterns {
                println!(
                    "  [{}] Don't: {} -> Instead: {}",
                    a.id, a.pattern, a.instead
                );
            }
            println!();
        }
    }

    Ok(())
}

pub struct KnowledgeAddArgs {
    pub entry_type: String,
    pub id: Option<String>,
    pub scope: Option<String>,
    pub rule: Option<String>,
    pub module: Option<String>,
    pub owns: Option<String>,
    pub boundary: Option<String>,
    pub pattern: Option<String>,
    pub instead: Option<String>,
    pub stability: Option<String>,
    pub decided_in: Option<String>,
    pub learned_from: Option<String>,
}

/// Run `git chronicle knowledge add`.
pub fn run_add(args: KnowledgeAddArgs) -> Result<()> {
    let KnowledgeAddArgs {
        entry_type,
        id,
        scope,
        rule,
        module,
        owns,
        boundary,
        pattern,
        instead,
        stability,
        decided_in,
        learned_from,
    } = args;
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let mut store =
        knowledge::read_store(&git_ops).map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    match entry_type.as_str() {
        "convention" => {
            let scope_val = scope.ok_or_else(|| crate::error::ChronicleError::Validation {
                message: "--scope is required for convention entries".to_string(),
                location: snafu::Location::default(),
            })?;
            let rule_val = rule.ok_or_else(|| crate::error::ChronicleError::Validation {
                message: "--rule is required for convention entries".to_string(),
                location: snafu::Location::default(),
            })?;
            let stability_val = parse_stability(stability.as_deref())?;
            let entry_id = id.unwrap_or_else(|| generate_id("conv", &store));
            store.conventions.push(Convention {
                id: entry_id.clone(),
                scope: scope_val,
                rule: rule_val,
                decided_in,
                stability: stability_val,
            });
            println!("Added convention: {entry_id}");
        }
        "boundary" => {
            let module_val = module.ok_or_else(|| crate::error::ChronicleError::Validation {
                message: "--module is required for boundary entries".to_string(),
                location: snafu::Location::default(),
            })?;
            let owns_val = owns.ok_or_else(|| crate::error::ChronicleError::Validation {
                message: "--owns is required for boundary entries".to_string(),
                location: snafu::Location::default(),
            })?;
            let boundary_val =
                boundary.ok_or_else(|| crate::error::ChronicleError::Validation {
                    message: "--boundary is required for boundary entries".to_string(),
                    location: snafu::Location::default(),
                })?;
            let entry_id = id.unwrap_or_else(|| generate_id("bound", &store));
            store.boundaries.push(ModuleBoundary {
                id: entry_id.clone(),
                module: module_val,
                owns: owns_val,
                boundary: boundary_val,
                decided_in,
            });
            println!("Added boundary: {entry_id}");
        }
        "anti-pattern" => {
            let pattern_val = pattern.ok_or_else(|| crate::error::ChronicleError::Validation {
                message: "--pattern is required for anti-pattern entries".to_string(),
                location: snafu::Location::default(),
            })?;
            let instead_val = instead.ok_or_else(|| crate::error::ChronicleError::Validation {
                message: "--instead is required for anti-pattern entries".to_string(),
                location: snafu::Location::default(),
            })?;
            let entry_id = id.unwrap_or_else(|| generate_id("ap", &store));
            store.anti_patterns.push(AntiPattern {
                id: entry_id.clone(),
                pattern: pattern_val,
                instead: instead_val,
                learned_from,
            });
            println!("Added anti-pattern: {entry_id}");
        }
        other => {
            return Err(crate::error::ChronicleError::Validation {
                message: format!(
                    "Unknown entry type '{other}'. Use: convention, boundary, anti-pattern"
                ),
                location: snafu::Location::default(),
            });
        }
    }

    knowledge::write_store(&git_ops, &store).map_err(|e| crate::error::ChronicleError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;

    Ok(())
}

/// Run `git chronicle knowledge remove`.
pub fn run_remove(id: String) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let mut store =
        knowledge::read_store(&git_ops).map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    if store.remove_by_id(&id) {
        knowledge::write_store(&git_ops, &store).map_err(|e| {
            crate::error::ChronicleError::Git {
                source: e,
                location: snafu::Location::default(),
            }
        })?;
        println!("Removed: {id}");
    } else {
        println!("Not found: {id}");
    }

    Ok(())
}

fn parse_stability(s: Option<&str>) -> Result<Stability> {
    match s {
        Some("permanent") | None => Ok(Stability::Permanent),
        Some("provisional") => Ok(Stability::Provisional),
        Some("experimental") => Ok(Stability::Experimental),
        Some(other) => Err(crate::error::ChronicleError::Validation {
            message: format!(
                "Unknown stability '{other}'. Use: permanent, provisional, experimental"
            ),
            location: snafu::Location::default(),
        }),
    }
}

fn generate_id(prefix: &str, store: &KnowledgeStore) -> String {
    let total = store.conventions.len() + store.boundaries.len() + store.anti_patterns.len();
    format!("{prefix}-{}", total + 1)
}
