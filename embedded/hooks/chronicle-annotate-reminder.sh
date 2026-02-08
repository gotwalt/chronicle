#!/usr/bin/env bash
# PostToolUse hook: remind agent to annotate after git commit
#
# This hook is invoked by Claude Code after the Bash tool is used.
# It checks if the command was a git commit and reminds the agent
# to annotate using the chronicle annotate CLI.

# Read the tool use input from stdin
input=$(cat)

# Extract the tool name and command
tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null)
command=$(echo "$input" | jq -r '.tool_input.command // empty' 2>/dev/null)

# Only proceed if this was a Bash tool call
if [ "$tool_name" != "Bash" ]; then
    exit 0
fi

# Check if the command was a git commit
if echo "$command" | grep -qE '(^|\s|&&|\|)git\s+commit(\s|$)'; then
    cat << 'MSG'
REMINDER: Annotate this commit. Annotations are context for future agents — write what the diff cannot tell you.

Default (any non-trivial commit — single command, no temp files):
  git chronicle annotate --live << 'EOF'
  {"commit":"HEAD","summary":"WHY this approach, not what changed","rejected_alternatives":[{"approach":"...","reason":"..."}],"decisions":[{"what":"...","why":"...","stability":"provisional"}]}
  EOF

Summary-only (trivial changes like typos, renames, dep bumps):
  git chronicle annotate --summary "WHY, not what — do not restate the commit message."
MSG
fi
