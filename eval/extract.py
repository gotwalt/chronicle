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


def _agent_base_tag(repo_dir: Path) -> str:
    """Return the best base tag for extraction.

    Prefers eval-agent-start (after chronicle install) over
    eval-setup-complete (before chronicle install).
    """
    result = subprocess.run(
        ["git", "tag", "-l", "eval-agent-start"],
        cwd=repo_dir,
        capture_output=True,
        text=True,
    )
    if "eval-agent-start" in result.stdout:
        return "eval-agent-start"
    return "eval-setup-complete"


def extract_commit_messages(repo_dir: Path) -> list[str]:
    """Get commit messages for agent commits (after chronicle install)."""
    tag = _agent_base_tag(repo_dir)
    output = run_git(
        ["log", f"{tag}..HEAD", "--format=%B%x00", "--reverse"],
        repo_dir,
    )
    if not output:
        return []
    messages = [m.strip() for m in output.split("\x00") if m.strip()]
    return messages


def extract_files_changed(repo_dir: Path) -> list[str]:
    """Get files changed by agent commits (after chronicle install)."""
    tag = _agent_base_tag(repo_dir)
    output = run_git(
        ["diff", "--name-only", f"{tag}..HEAD"],
        repo_dir,
    )
    if not output:
        return []
    return [f for f in output.splitlines() if f.strip()]


def extract_diff_text(repo_dir: Path) -> str:
    """Get the full diff of agent commits (after chronicle install)."""
    tag = _agent_base_tag(repo_dir)
    return run_git(["diff", f"{tag}..HEAD"], repo_dir)
