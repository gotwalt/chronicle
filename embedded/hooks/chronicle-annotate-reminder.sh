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
    echo "REMINDER: You just created a git commit. Annotate it by piping AnnotateInput JSON to: git chronicle annotate --live. Include commit: \"HEAD\", summary, and narrative context. See the annotate skill for details."
fi
