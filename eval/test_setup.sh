#!/usr/bin/env bash
# Quick test that setup scripts work and tests show expected failures
set -euo pipefail

cd /Users/aaron/src/git-chronicle

echo "=== Testing setup scripts ==="

for task in circular-config cache-invalidation retry-backoff; do
    echo ""
    echo "--- $task ---"
    rm -rf "/tmp/test-$task"
    bash "eval/tasks/$task/setup.sh" "/tmp/test-$task"

    echo "Running tests (failures expected for bug-revealing tests):"
    cd "/tmp/test-$task"
    uv run python -m pytest tests/ -v 2>&1 || true
    cd /Users/aaron/src/git-chronicle
done

echo ""
echo "=== Testing dry-run ==="
uv run python -m eval --dry-run

echo ""
echo "=== All tests complete ==="
