"""Post-run extraction of annotations and git metadata from a task repo."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

from eval.models import Annotation, WisdomEntry


def run_git(args: list[str], cwd: Path) -> str:
    result = subprocess.run(
        ["git"] + args,
        cwd=cwd,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed: {result.stderr.strip()}"
        )
    return result.stdout.strip()


def extract_annotations(repo_dir: Path, chronicle_binary: str) -> list[Annotation]:
    """Run chronicle export and parse JSONL into Annotation objects."""
    result = subprocess.run(
        [chronicle_binary, "export"],
        cwd=repo_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        # No annotations is not an error — agent may not have annotated
        return []

    annotations = []
    for line in result.stdout.strip().splitlines():
        if not line:
            continue
        entry = json.loads(line)
        ann_data = entry.get("annotation", {})
        wisdom_entries = []
        for w in ann_data.get("wisdom", []):
            wisdom_entries.append(WisdomEntry(
                category=w.get("category", ""),
                content=w.get("content", ""),
                file=w.get("file"),
            ))
        annotations.append(Annotation(
            commit_sha=entry.get("commit_sha", ""),
            timestamp=entry.get("timestamp", ""),
            summary=ann_data.get("summary", ""),
            wisdom=wisdom_entries,
        ))
    return annotations


def extract_commit_messages(repo_dir: Path) -> list[str]:
    """Get commit messages after the eval-setup-complete tag."""
    output = run_git(
        ["log", "eval-setup-complete..HEAD", "--format=%B", "--reverse"],
        repo_dir,
    )
    if not output:
        return []
    # Split on double-newline boundaries between commits
    messages = [m.strip() for m in output.split("\n\n") if m.strip()]
    return messages


def extract_files_changed(repo_dir: Path) -> list[str]:
    """Get files changed in commits after eval-setup-complete."""
    output = run_git(
        ["diff", "--name-only", "eval-setup-complete..HEAD"],
        repo_dir,
    )
    if not output:
        return []
    return [f for f in output.splitlines() if f.strip()]


def extract_diff_text(repo_dir: Path) -> str:
    """Get the full diff from eval-setup-complete to HEAD."""
    return run_git(["diff", "eval-setup-complete..HEAD"], repo_dir)
