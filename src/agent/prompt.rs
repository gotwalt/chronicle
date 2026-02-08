use crate::annotate::gather::AnnotationContext;

/// Build the system prompt for the annotation agent.
pub fn build_system_prompt(context: &AnnotationContext) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are an expert code annotator for the Chronicle system. Your role is to analyze \
         code changes in a git commit and produce structured annotations that capture the **story** \
         behind the change: why this approach was chosen, what was considered and rejected, and \
         what is non-obvious about the code.\n\n",
    );

    prompt.push_str(
        "## Schema: chronicle/v2 (narrative-first)\n\n\
         The commit is the primary unit of annotation. Your job is to tell the story:\n\n\
         1. **Narrative** (required, commit-level): What this commit does and WHY this approach. \
         Not a diff restatement. Include motivation and rejected alternatives if known.\n\
         2. **Decisions** (optional): Architectural or design decisions made in this commit, \
         with stability level (permanent, provisional, experimental).\n\
         3. **Code markers** (optional): Only flag non-obvious code behavior:\n\
         - `contract`: behavioral invariants or preconditions\n\
         - `hazard`: something that could cause bugs if misunderstood\n\
         - `dependency`: code that assumes something about code elsewhere\n\
         - `unstable`: provisional code that should be revisited\n\n\
         DO NOT annotate every function. Only emit markers where there is something \
         genuinely non-obvious that a future developer needs to know.\n\n",
    );

    // Include author context instructions based on whether it's present
    if let Some(author_ctx) = &context.author_context {
        prompt.push_str(
            "## Context Level: Enhanced\n\n\
             The commit author provided context about this change. Weight this information \
             heavily in your annotations:\n\n",
        );
        if let Some(task) = &author_ctx.task {
            prompt.push_str(&format!("- **Task**: {task}\n"));
        }
        if let Some(reasoning) = &author_ctx.reasoning {
            prompt.push_str(&format!("- **Author reasoning**: {reasoning}\n"));
        }
        if let Some(deps) = &author_ctx.dependencies {
            prompt.push_str(&format!("- **Dependencies noted**: {deps}\n"));
        }
        if !author_ctx.tags.is_empty() {
            prompt.push_str(&format!("- **Tags**: {}\n", author_ctx.tags.join(", ")));
        }
        prompt.push('\n');
        prompt.push_str(
            "Use the author's reasoning as the primary basis for the narrative. \
             The author's notes about alternatives and constraints are high-value — \
             include them as rejected_alternatives and decisions.\n\n",
        );
    } else {
        prompt.push_str(
            "## Context Level: Inferred\n\n\
             No author context was provided. Be conservative:\n\
             - Focus on what is clearly evident from the code and commit message\n\
             - Mark contracts as `inferred` rather than `author`\n\
             - Avoid speculating about motivation when it is not clear\n\
             - Still identify genuine hazards and dependencies\n\n",
        );
    }

    prompt.push_str(&format!(
        "## Commit Message\n\n```\n{}\n```\n\n",
        context.commit_message
    ));

    prompt.push_str(
        "## Instructions\n\n\
         1. Use `get_diff` to examine the full diff\n\
         2. Use `get_file_content` to understand the changed files\n\
         3. Use `get_commit_info` if you need additional commit metadata\n\
         4. Call `emit_narrative` ONCE with the commit-level story (required)\n\
         5. Call `emit_decision` for each architectural/design decision (if any)\n\
         6. Call `emit_marker` ONLY for genuinely non-obvious code (contracts, hazards, dependencies)\n\
         7. After emitting, provide a brief summary\n\n\
         Most commits need only `emit_narrative`. A typical commit produces 1-3 tool calls total, \
         not one per function.\n",
    );

    prompt
}
