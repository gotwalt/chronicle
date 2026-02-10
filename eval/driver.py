"""Core orchestration for eval runs.

Sets up task repos, installs Chronicle, runs agents, extracts results.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from eval.extract import (
    extract_annotations,
    extract_commit_messages,
    extract_diff_text,
    extract_files_changed,
)
from eval.models import RunResult, TaskConfig
from eval.scoring import ScoreReport, score_run

# Project root (parent of eval/)
PROJECT_ROOT = Path(__file__).resolve().parent.parent


def load_task_config(task_id: str) -> TaskConfig:
    """Load a task config from eval/tasks/<task_id>/task.toml."""
    import tomllib

    task_dir = PROJECT_ROOT / "eval" / "tasks" / task_id
    toml_path = task_dir / "task.toml"

    with open(toml_path, "rb") as f:
        data = tomllib.load(f)

    from eval.models import GroundTruth

    ground_truth = []
    for gt in data.get("ground_truth", []):
        ground_truth.append(GroundTruth(
            category=gt["category"],
            content=gt["content"],
            tier=gt["tier"],
            discoverable_via=gt["discoverable_via"],
        ))

    return TaskConfig(
        id=data["task"]["id"],
        name=data["task"]["name"],
        difficulty=data["task"]["difficulty"],
        prompt=data["instructions"]["prompt"],
        init_script=str(task_dir / data["setup"]["init_script"]),
        ground_truth=ground_truth,
    )


def load_eval_config(config_path: Path | None = None) -> dict:
    """Load eval/config.toml."""
    import tomllib

    if config_path is None:
        config_path = PROJECT_ROOT / "eval" / "config.toml"

    with open(config_path, "rb") as f:
        return tomllib.load(f)


def setup_task_repo(task_config: TaskConfig, work_dir: Path) -> Path:
    """Create a fresh task repo by running the setup script.

    Returns the repo directory path.
    """
    repo_dir = work_dir / task_config.id
    result = subprocess.run(
        ["bash", task_config.init_script, str(repo_dir)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Setup script failed for {task_config.id}: {result.stderr}"
        )

    # Verify repo was created correctly
    git_dir = repo_dir / ".git"
    if not git_dir.is_dir():
        raise RuntimeError(f"Setup script didn't create .git in {repo_dir}")

    # Verify tag exists
    tag_check = subprocess.run(
        ["git", "tag", "-l", "eval-setup-complete"],
        cwd=repo_dir,
        capture_output=True,
        text=True,
    )
    if "eval-setup-complete" not in tag_check.stdout:
        raise RuntimeError(
            f"Setup script didn't create eval-setup-complete tag in {repo_dir}"
        )

    return repo_dir


def install_chronicle(
    repo_dir: Path,
    chronicle_binary: str,
    prompt_path: Path,
) -> None:
    """Install Chronicle skills and CLAUDE.md into the task repo.

    - Copies the prompt variant as the annotate skill
    - Copies the context skill
    - Writes CLAUDE.md from the embedded snippet
    - Rewrites `git chronicle` references to use the absolute binary path
    """
    abs_binary = str(Path(chronicle_binary).resolve())

    # Create .claude/skills directories
    skills_dir = repo_dir / ".claude" / "skills"
    annotate_dir = skills_dir / "annotate"
    context_dir = skills_dir / "context"
    annotate_dir.mkdir(parents=True, exist_ok=True)
    context_dir.mkdir(parents=True, exist_ok=True)

    # Copy and rewrite annotate skill
    prompt_text = prompt_path.read_text()
    prompt_text = _rewrite_binary_refs(prompt_text, abs_binary)
    (annotate_dir / "SKILL.md").write_text(prompt_text)

    # Copy and rewrite context skill
    context_src = PROJECT_ROOT / ".claude" / "skills" / "context" / "SKILL.md"
    context_text = context_src.read_text()
    context_text = _rewrite_binary_refs(context_text, abs_binary)
    (context_dir / "SKILL.md").write_text(context_text)

    # Write CLAUDE.md from embedded snippet
    snippet_src = PROJECT_ROOT / "embedded" / "claude-md-snippet.md"
    snippet_text = snippet_src.read_text()
    snippet_text = _rewrite_binary_refs(snippet_text, abs_binary)
    (repo_dir / ".claude" / "CLAUDE.md").write_text(snippet_text)

    # Commit the chronicle setup so it's available to the agent
    subprocess.run(
        ["git", "add", "-A"],
        cwd=repo_dir,
        capture_output=True,
    )
    subprocess.run(
        ["git", "commit", "-m", "Add Chronicle annotation skills"],
        cwd=repo_dir,
        capture_output=True,
        env={
            **os.environ,
            "GIT_AUTHOR_DATE": "2025-01-15T10:01:00+00:00",
            "GIT_COMMITTER_DATE": "2025-01-15T10:01:00+00:00",
        },
    )


def _rewrite_binary_refs(text: str, abs_binary: str) -> str:
    """Replace `git chronicle` and `git-chronicle` with the absolute binary path."""
    text = re.sub(r"git chronicle\b", abs_binary, text)
    text = re.sub(r"git-chronicle\b", abs_binary, text)
    return text


def build_agent_prompt(task_config: TaskConfig, chronicle_binary: str) -> str:
    """Build the full prompt sent to the agent."""
    abs_binary = str(Path(chronicle_binary).resolve())

    return f"""You are working on a Python project. Here is your task:

{task_config.prompt.strip()}

After fixing the bug and verifying tests pass, commit your changes with a
descriptive commit message.

Then annotate the commit using Chronicle. Use the annotate skill in
.claude/skills/annotate/SKILL.md for guidance. Here is the command pattern:

```bash
{abs_binary} annotate --live << 'EOF'
{{
  "commit": "HEAD",
  "summary": "WHY you chose this approach (not what you changed)",
  "wisdom": [
    {{
      "category": "dead_end|gotcha|insight|unfinished_thread",
      "content": "What you learned that isn't visible in the code",
      "file": "path/to/relevant/file.py"
    }}
  ]
}}
EOF
```

Capture what you learned during this task — especially:
- Approaches you tried that didn't work (dead_end)
- Non-obvious traps or constraints (gotcha)
- Key insights about the codebase (insight)
- Anything left unfinished or uncertain (unfinished_thread)
"""


def run_agent(
    repo_dir: Path,
    prompt: str,
    model: str,
    max_budget_usd: float,
    timeout_seconds: int,
) -> tuple[str, float]:
    """Run the Claude Code agent on the task repo.

    Returns (stdout_text, elapsed_seconds).
    """
    cmd = [
        "claude",
        "-p", prompt,
        "--model", model,
        "--output-format", "text",
        "--permission-mode", "bypassPermissions",
        "--max-budget-usd", str(max_budget_usd),
        "--tools", "Bash,Read,Write,Edit,Glob,Grep",
    ]

    start = time.time()
    result = subprocess.run(
        cmd,
        cwd=repo_dir,
        capture_output=True,
        text=True,
        timeout=timeout_seconds,
    )
    elapsed = time.time() - start

    output = result.stdout
    if result.stderr:
        output += "\n--- STDERR ---\n" + result.stderr

    return output, elapsed


def run_single(
    task_id: str,
    prompt_variant: str,
    config: dict,
    work_dir: Path | None = None,
) -> tuple[RunResult, ScoreReport]:
    """Full pipeline: setup -> run agent -> extract -> score.

    Returns (RunResult, ScoreReport).
    """
    task_config = load_task_config(task_id)
    eval_config = config

    chronicle_binary = eval_config["chronicle"]["binary"]
    # Resolve relative to project root
    if not os.path.isabs(chronicle_binary):
        chronicle_binary = str(PROJECT_ROOT / chronicle_binary)

    model = eval_config["agent"]["model"]
    max_budget = eval_config["agent"]["max_budget_usd"]
    timeout = eval_config["agent"]["timeout_seconds"]

    prompt_path = PROJECT_ROOT / "eval" / "prompts" / f"{prompt_variant}.md"
    if not prompt_path.exists():
        raise FileNotFoundError(f"Prompt variant not found: {prompt_path}")

    cleanup = False
    if work_dir is None:
        work_dir = Path(tempfile.mkdtemp(prefix="chronicle-eval-"))
        cleanup = True

    try:
        # Setup
        repo_dir = setup_task_repo(task_config, work_dir)
        install_chronicle(repo_dir, chronicle_binary, prompt_path)

        # Run agent
        prompt = build_agent_prompt(task_config, chronicle_binary)
        agent_output, elapsed = run_agent(
            repo_dir, prompt, model, max_budget, timeout,
        )

        # Extract results
        annotations = extract_annotations(repo_dir, chronicle_binary)
        commit_messages = extract_commit_messages(repo_dir)
        files_changed = extract_files_changed(repo_dir)
        diff_text = extract_diff_text(repo_dir)

        run_result = RunResult(
            task_id=task_id,
            prompt_variant=prompt_variant,
            annotations=annotations,
            commit_messages=commit_messages,
            files_changed=files_changed,
            diff_text=diff_text,
            agent_output=agent_output,
            elapsed_seconds=elapsed,
            success=True,
        )

    except Exception as e:
        run_result = RunResult(
            task_id=task_id,
            prompt_variant=prompt_variant,
            annotations=[],
            commit_messages=[],
            files_changed=[],
            diff_text="",
            agent_output="",
            elapsed_seconds=0.0,
            success=False,
            error=str(e),
        )

    finally:
        if cleanup and work_dir.exists():
            shutil.rmtree(work_dir, ignore_errors=True)

    score_report = score_run(run_result)
    return run_result, score_report
