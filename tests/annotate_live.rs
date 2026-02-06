//! Integration test: call handle_annotate against the real repo to write a
//! git note for the HEAD commit via the live path (zero LLM cost).

use chronicle::git::CliOps;
use chronicle::mcp::annotate_handler::{
    AnchorInput, AnnotateInput, ConstraintInput, RegionInput, handle_annotate,
};
use chronicle::schema::{CrossCuttingConcern, CrossCuttingRegionRef, LineRange, SemanticDependency};

#[test]
fn annotate_head_commit() {
    let repo_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let git_ops = CliOps::new(repo_dir);

    let input = AnnotateInput {
        commit: "HEAD".to_string(),
        summary: "MVP write path implementation with MCP annotate handler. Implements the full \
            chronicle CLI framework, git operations layer, tree-sitter AST parsing, Anthropic LLM \
            provider, writing agent, hooks, pre-LLM filtering, and annotation schema. Adds MCP \
            annotate handler (live path) for zero-cost agent-authored annotations with AST anchor \
            resolution, quality checks, validation, and git notes write."
            .to_string(),
        task: Some("MVP+MCP-annotate".to_string()),
        regions: vec![
            RegionInput {
                file: "src/mcp/annotate_handler.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "handle_annotate".to_string(),
                },
                lines: LineRange { start: 110, end: 160 },
                intent: "Core MCP annotate handler — receives structured annotation data from the \
                    calling agent, resolves AST anchors to correct line ranges and fill signatures, \
                    validates the annotation, and writes it as a git note."
                    .to_string(),
                reasoning: Some(
                    "Separated from the batch path (agent loop) because the calling agent already \
                    knows intent, reasoning, and constraints. No LLM call needed. The handler \
                    reuses existing AST resolution and schema validation."
                        .to_string(),
                ),
                constraints: vec![
                    ConstraintInput {
                        text: "Must always set context_level to Enhanced and ConstraintSource to \
                            Author since the authoring agent has direct knowledge."
                            .to_string(),
                    },
                    ConstraintInput {
                        text: "Quality warnings are non-blocking — they're returned to the caller \
                            but don't prevent the note from being written."
                            .to_string(),
                    },
                    ConstraintInput {
                        text: "Validation errors (empty summary, invalid line ranges) must reject \
                            the annotation entirely — no partial writes."
                            .to_string(),
                    },
                ],
                semantic_dependencies: vec![
                    SemanticDependency {
                        file: "src/ast/mod.rs".to_string(),
                        anchor: "resolve_anchor".to_string(),
                        nature: "Delegates anchor resolution; assumes it returns None on no match \
                            rather than erroring."
                            .to_string(),
                    },
                    SemanticDependency {
                        file: "src/schema/annotation.rs".to_string(),
                        anchor: "Annotation::validate".to_string(),
                        nature: "Calls validate() to enforce structural correctness before writing."
                            .to_string(),
                    },
                    SemanticDependency {
                        file: "src/git/mod.rs".to_string(),
                        anchor: "GitOps::note_write".to_string(),
                        nature: "Writes the serialized annotation as a git note."
                            .to_string(),
                    },
                ],
                tags: vec!["mcp".to_string(), "live-path".to_string(), "core".to_string()],
                risk_notes: None,
            },
            RegionInput {
                file: "src/mcp/annotate_handler.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "resolve_and_build_region".to_string(),
                },
                lines: LineRange { start: 165, end: 250 },
                intent: "Resolve a single region's anchor against the AST outline and build the \
                    final RegionAnnotation with corrected lines and filled signature."
                    .to_string(),
                reasoning: Some(
                    "Graceful degradation: if the language is unsupported, the file is missing, \
                    or the anchor doesn't resolve, the handler falls back to using the input \
                    as-is rather than failing the entire annotation."
                        .to_string(),
                ),
                constraints: vec![ConstraintInput {
                    text: "Must never fail the entire annotation due to a single region's anchor \
                        not resolving — fallback to input lines and unresolved status."
                        .to_string(),
                }],
                semantic_dependencies: vec![],
                tags: vec!["ast".to_string(), "anchor-resolution".to_string()],
                risk_notes: None,
            },
            RegionInput {
                file: "src/mcp/annotate_handler.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "check_quality".to_string(),
                },
                lines: LineRange { start: 85, end: 108 },
                intent: "Non-blocking quality feedback — warns the caller about short intent, \
                    missing reasoning, or absent constraints without preventing the write."
                    .to_string(),
                reasoning: Some(
                    "Quality enforcement should nudge toward better annotations without blocking \
                    work. The warnings are returned in the result for the agent to improve next time."
                        .to_string(),
                ),
                constraints: vec![],
                semantic_dependencies: vec![],
                tags: vec!["quality".to_string()],
                risk_notes: None,
            },
            RegionInput {
                file: "src/error.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "enum".to_string(),
                    name: "ChronicleError".to_string(),
                },
                lines: LineRange { start: 68, end: 75 },
                intent: "Add Validation variant for annotation structural errors caught by \
                    the MCP handler before writing."
                    .to_string(),
                reasoning: Some(
                    "Reuses the existing snafu error pattern with module() attribute. Validation \
                    is a distinct error category from Git, Provider, or Agent errors."
                        .to_string(),
                ),
                constraints: vec![ConstraintInput {
                    text: "Variant name must not end in 'Error' to avoid collision with the \
                        enum name per snafu module() convention."
                        .to_string(),
                }],
                semantic_dependencies: vec![],
                tags: vec!["error-handling".to_string()],
                risk_notes: None,
            },
        ],
        cross_cutting: vec![CrossCuttingConcern {
            description: "Two-path annotation architecture: live path (MCP handler, zero LLM cost, \
                agent provides metadata directly) and batch path (agent loop with API calls, for \
                CI and backfill). Both produce identical Annotation schema output stored as git notes."
                .to_string(),
            regions: vec![
                CrossCuttingRegionRef {
                    file: "src/mcp/annotate_handler.rs".to_string(),
                    anchor: "handle_annotate".to_string(),
                },
                CrossCuttingRegionRef {
                    file: "src/annotate/mod.rs".to_string(),
                    anchor: "run".to_string(),
                },
            ],
            tags: vec!["architecture".to_string()],
        }],
    };

    let result = handle_annotate(&git_ops, input).unwrap();

    assert!(result.success);
    assert_eq!(result.regions_written, 4);
    println!("Commit: {}", result.commit);
    println!("Regions written: {}", result.regions_written);
    println!("Warnings: {:?}", result.warnings);
    for ar in &result.anchor_resolutions {
        println!(
            "  {} / {} → {:?}",
            ar.file, ar.requested_name, ar.resolution
        );
    }
}
