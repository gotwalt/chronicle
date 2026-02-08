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

# Session deduplication — hint once per file per day
HINT_CACHE="/tmp/chronicle-hints-$(date +%Y%m%d)"
if grep -qxF "$file_path" "$HINT_CACHE" 2>/dev/null; then
    exit 0
fi
echo "$file_path" >> "$HINT_CACHE"

# Knowledge hint — once per session
KNOWLEDGE_KEY="__chronicle_knowledge_hint__"
if ! grep -qxF "$KNOWLEDGE_KEY" "$HINT_CACHE" 2>/dev/null; then
    echo "$KNOWLEDGE_KEY" >> "$HINT_CACHE"
    echo "TIP: Repo-level conventions and anti-patterns may apply. Check: git chronicle knowledge list"
fi

# Only for source code files
case "$file_path" in
    *.rs|*.ts|*.tsx|*.js|*.jsx|*.py|*.go|*.java|*.cpp|*.c|*.h)
        echo "TIP: Previous agents may have left contracts, decisions, or warnings for $(basename "$file_path"). Check before modifying: git chronicle contracts \"$file_path\""
        ;;
esac
