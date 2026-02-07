use crate::annotate::gather::AnnotationContext;

/// Build the system prompt for the annotation agent.
pub fn build_system_prompt(context: &AnnotationContext) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are an expert code annotator for the chronicle system. Your role is to analyze \
         code changes in a git commit and produce structured annotations that capture the intent, \
         reasoning, and constraints behind each change.\n\n",
    );

    prompt.push_str(
        "## Schema: chronicle/v1\n\n\
         Each annotation describes a **region** (a semantic unit of change) with:\n\
         - `file`: the file path\n\
         - `ast_anchor`: identifies the semantic unit (unit_type, name, optional signature)\n\
         - `lines`: start and end line numbers in the new file\n\
         - `intent`: a clear description of what this change does and why\n\
         - `reasoning`: (optional) deeper explanation of the approach\n\
         - `constraints`: (optional) invariants or requirements this change must satisfy\n\
         - `semantic_dependencies`: (optional) other code this change depends on\n\
         - `tags`: (optional) categorical labels like \"refactor\", \"bugfix\", \"feature\"\n\
         - `risk_notes`: (optional) potential risks or concerns\n\n",
    );

    prompt.push_str(
        "Cross-cutting concerns span multiple regions and describe patterns like \
         \"error handling changes across all API endpoints\".\n\n",
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
            "Use the author's reasoning and constraints as the primary basis for annotation. \
             Add inferred information only to supplement what the author provided.\n\n",
        );
    } else {
        prompt.push_str(
            "## Context Level: Inferred\n\n\
             No author context was provided. Be conservative in your annotations:\n\
             - Focus on what is clearly evident from the code\n\
             - Mark constraints as `inferred` rather than `author`\n\
             - Avoid speculating about intent when it is not clear from the diff\n\n",
        );
    }

    prompt.push_str(&format!(
        "## Commit Message\n\n```\n{}\n```\n\n",
        context.commit_message
    ));

    prompt.push_str(
        "## Instructions\n\n\
         1. Use `get_diff` to examine the full diff\n\
         2. Use `get_file_content` and `get_ast_outline` to understand the changed files\n\
         3. Use `get_commit_info` if you need additional commit metadata\n\
         4. Emit one `emit_annotation` call per changed semantic unit (function, struct, impl block, etc.)\n\
         5. If changes span multiple files with a common theme, also emit `emit_cross_cutting`\n\
         6. Be precise with line ranges and AST anchors\n\
         7. Write clear, concise intent descriptions\n\
         8. After emitting all annotations, provide a brief summary of the overall change\n"
    );

    prompt
}
