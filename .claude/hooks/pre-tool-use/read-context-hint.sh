#!/usr/bin/env bash
# PreToolUse hook: suggest reading chronicle annotations before editing
#
# This hook is invoked by Claude Code before the Edit or Write tool is used.
# It reminds the agent that Chronicle annotations may exist for the file
# being modified, and suggests reading them first.

input=$(cat)
tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null)

# Only for Edit and Write tools
case "$tool_name" in
    Edit|Write) ;;
    *) exit 0 ;;
esac

file_path=$(echo "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

# Only for source code files
case "$file_path" in
    *.rs|*.ts|*.tsx|*.js|*.jsx|*.py|*.go|*.java|*.cpp|*.c|*.h)
        echo "TIP: Consider reading Chronicle annotations for $(basename "$file_path") before modifying it: ./target/debug/git-chronicle read \"$file_path\""
        ;;
esac
