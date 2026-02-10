"""LLM-as-judge scoring for Chronicle annotation quality.

Uses the Anthropic Python SDK to rate individual wisdom entries and measure
coverage against planted ground truth. This is the primary iteration tool
for prompt experiments.
"""

from __future__ import annotations

import json
import os
import re
import sys
import time
from dataclasses import asdict
from pathlib import Path
from typing import Optional

from eval.models import (
    CoverageResult,
    LLMJudgeScores,
    RunResult,
    ScoreReport,
    WisdomQualityScore,
)

# ---------------------------------------------------------------------------
# Prompt templates
# ---------------------------------------------------------------------------

QUALITY_SYSTEM = """\
You are an expert evaluator of software engineering annotations. You assess \
whether a "wisdom entry" — a structured note about a code change — adds genuine \
value for future developers or agents working on the same codebase.

Rate the entry on five dimensions (1 = worst, 5 = best):

- **redundancy** (1 = restates the diff/commit message, 5 = entirely new information)
- **specificity** (1 = vague platitude, 5 = precise reference to code/behavior)
- **actionability** (1 = no clear takeaway, 5 = reader knows exactly what to do/avoid)
- **depth** (1 = surface observation, 5 = non-obvious insight requiring real reasoning)
- **accuracy** (1 = factually wrong, 5 = verifiably correct from the diff)

Then classify the entry:
- **high_value**: Saves the next developer real time or prevents a real bug
- **moderate_value**: Useful context but somewhat obvious from reading the code
- **low_value**: Technically correct but adds little beyond the diff
- **noise**: Wrong, misleading, or pure restatement of the commit message

Respond with ONLY a JSON object (no markdown fences, no commentary):
{
  "redundancy": <int 1-5>,
  "specificity": <int 1-5>,
  "actionability": <int 1-5>,
  "depth": <int 1-5>,
  "accuracy": <int 1-5>,
  "classification": "<high_value|moderate_value|low_value|noise>",
  "reasoning": "<1-2 sentences explaining your rating>"
}"""

QUALITY_USER_TEMPLATE = """\
## Task
The agent was given this task:
{task_prompt}

## Commit Messages
{commit_messages}

## Diff (truncated)
{diff}

## Wisdom Entry to Evaluate
Category: {category}
File: {file}
Content: {content}"""

COVERAGE_SYSTEM = """\
You are evaluating whether an agent's annotations cover the key insights \
("ground truth") that an ideal agent would capture for a given task.

For each ground truth item, determine coverage:
- **full**: The annotations capture the same insight (possibly worded differently)
- **partial**: The annotations touch on the topic but miss the key point or lack precision
- **missed**: The annotations do not address this insight at all

Respond with ONLY a JSON object (no markdown fences, no commentary):
{
  "items": [
    {
      "ground_truth_index": <int>,
      "coverage": "<full|partial|missed>",
      "matched_entry": <int index of best matching annotation entry, or null>,
      "explanation": "<1 sentence>"
    }
  ]
}"""

COVERAGE_USER_TEMPLATE = """\
## Task
The agent was given this task:
{task_prompt}

## Ground Truth Items
{ground_truth}

## Agent's Annotations
{annotations}"""


# ---------------------------------------------------------------------------
# JSON parsing helpers
# ---------------------------------------------------------------------------


def _parse_json_response(text: str) -> dict:
    """Extract JSON from an LLM response, stripping markdown fences if present."""
    # Strip markdown code fences
    stripped = re.sub(r"^```(?:json)?\s*\n?", "", text.strip())
    stripped = re.sub(r"\n?```\s*$", "", stripped)

    # Try direct parse first
    try:
        return json.loads(stripped)
    except json.JSONDecodeError:
        pass

    # Try to extract the first JSON object
    match = re.search(r"\{.*\}", stripped, re.DOTALL)
    if match:
        try:
            return json.loads(match.group())
        except json.JSONDecodeError:
            pass

    raise ValueError(f"Could not parse JSON from response: {text[:200]}")


def _truncate_diff(diff: str, max_chars: int) -> str:
    """Truncate diff to max_chars, filtering .claude/ setup noise first."""
    lines = diff.split("\n")
    filtered = []
    skip = False
    for line in lines:
        # Skip diffs for .claude/ files (setup noise)
        if line.startswith("diff --git") and "/.claude/" in line:
            skip = True
            continue
        if line.startswith("diff --git") and "/.claude/" not in line:
            skip = False
        if not skip:
            filtered.append(line)

    result = "\n".join(filtered)
    if len(result) > max_chars:
        return result[:max_chars] + "\n... [truncated]"
    return result


def _format_ground_truth(items: list[dict]) -> str:
    """Format ground truth items as a numbered list for the coverage prompt."""
    parts = []
    for i, item in enumerate(items):
        parts.append(
            f"{i}. [{item['tier']}] ({item['category']}) {item['content']}"
        )
    return "\n".join(parts)


def _format_annotations(annotations: list[dict]) -> str:
    """Format all annotations + wisdom entries for the coverage prompt."""
    parts = []
    entry_idx = 0
    for ann in annotations:
        parts.append(f"### Annotation (commit {ann.get('commit_sha', 'unknown')[:8]})")
        parts.append(f"Summary: {ann.get('summary', '(none)')}")
        for w in ann.get("wisdom", []):
            parts.append(
                f"  Entry {entry_idx}: [{w.get('category', '?')}] "
                f"(file: {w.get('file', 'none')}) {w.get('content', '')}"
            )
            entry_idx += 1
    return "\n".join(parts)


# ---------------------------------------------------------------------------
# AnthropicJudge
# ---------------------------------------------------------------------------


class AnthropicJudge:
    """Thin wrapper around the Anthropic SDK for judge calls."""

    def __init__(self, model: str = "claude-sonnet-4-5", max_retries: int = 3):
        api_key = os.environ.get("ANTHROPIC_API_KEY")
        if not api_key:
            raise RuntimeError(
                "ANTHROPIC_API_KEY not set. Set it in your environment or use --no-judge."
            )

        import anthropic

        self.client = anthropic.Anthropic(
            api_key=api_key,
            max_retries=max_retries,
        )
        self.model = model

    def _call_json(self, system: str, user: str) -> dict:
        """Send a message and parse the JSON response."""
        response = self.client.messages.create(
            model=self.model,
            max_tokens=1024,
            system=system,
            messages=[{"role": "user", "content": user}],
        )
        text = response.content[0].text
        return _parse_json_response(text)


# ---------------------------------------------------------------------------
# Quality judging
# ---------------------------------------------------------------------------


def _judge_quality_entry(
    judge: AnthropicJudge,
    entry_index: int,
    category: str,
    content: str,
    file: str,
    task_prompt: str,
    commit_messages: list[str],
    diff: str,
    diff_max_chars: int,
) -> Optional[WisdomQualityScore]:
    """Judge a single wisdom entry. Returns None on failure."""
    user_msg = QUALITY_USER_TEMPLATE.format(
        task_prompt=task_prompt.strip(),
        commit_messages="\n".join(f"- {m}" for m in commit_messages),
        diff=_truncate_diff(diff, diff_max_chars),
        category=category,
        file=file or "(none)",
        content=content,
    )

    try:
        result = judge._call_json(QUALITY_SYSTEM, user_msg)
        return WisdomQualityScore(
            entry_index=entry_index,
            category=category,
            redundancy=int(result.get("redundancy", 3)),
            specificity=int(result.get("specificity", 3)),
            actionability=int(result.get("actionability", 3)),
            depth=int(result.get("depth", 3)),
            accuracy=int(result.get("accuracy", 3)),
            classification=result.get("classification", "moderate_value"),
            reasoning=result.get("reasoning", ""),
        )
    except Exception as e:
        print(f"  Warning: quality judge failed for entry {entry_index}: {e}",
              file=sys.stderr)
        return None


# ---------------------------------------------------------------------------
# Coverage judging
# ---------------------------------------------------------------------------


def _judge_coverage(
    judge: AnthropicJudge,
    ground_truth: list[dict],
    annotations: list[dict],
    task_prompt: str,
) -> list[CoverageResult]:
    """Judge coverage of all ground truth items. Returns results list."""
    if not ground_truth:
        return []

    user_msg = COVERAGE_USER_TEMPLATE.format(
        task_prompt=task_prompt.strip(),
        ground_truth=_format_ground_truth(ground_truth),
        annotations=_format_annotations(annotations),
    )

    try:
        result = judge._call_json(COVERAGE_SYSTEM, user_msg)
        items = result.get("items", [])
        coverage_results = []
        for item in items:
            gt_idx = int(item.get("ground_truth_index", 0))
            tier = ground_truth[gt_idx]["tier"] if gt_idx < len(ground_truth) else "unknown"
            coverage_results.append(CoverageResult(
                ground_truth_index=gt_idx,
                tier=tier,
                coverage=item.get("coverage", "missed"),
                matched_entry=item.get("matched_entry"),
                explanation=item.get("explanation", ""),
            ))
        # Fill in any missing ground truth items as "missed"
        covered_indices = {r.ground_truth_index for r in coverage_results}
        for i, gt in enumerate(ground_truth):
            if i not in covered_indices:
                coverage_results.append(CoverageResult(
                    ground_truth_index=i,
                    tier=gt["tier"],
                    coverage="missed",
                    matched_entry=None,
                    explanation="Not returned by judge; assumed missed.",
                ))
        return coverage_results
    except Exception as e:
        print(f"  Warning: coverage judge failed: {e}", file=sys.stderr)
        # Return all missed on failure
        return [
            CoverageResult(
                ground_truth_index=i,
                tier=gt["tier"],
                coverage="missed",
                matched_entry=None,
                explanation=f"Judge call failed: {e}",
            )
            for i, gt in enumerate(ground_truth)
        ]


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------


def _aggregate_scores(
    quality_scores: list[WisdomQualityScore],
    coverage_results: list[CoverageResult],
    judge_model: str,
) -> LLMJudgeScores:
    """Aggregate per-entry scores into summary metrics."""

    def _mean(vals: list[int]) -> float:
        return sum(vals) / len(vals) if vals else 0.0

    def _tier_fractions(
        results: list[CoverageResult], tier: str
    ) -> tuple[float, float]:
        """Return (any_coverage_fraction, full_coverage_fraction) for a tier."""
        tier_items = [r for r in results if r.tier == tier]
        if not tier_items:
            return 0.0, 0.0
        any_cov = sum(1 for r in tier_items if r.coverage in ("full", "partial"))
        full_cov = sum(1 for r in tier_items if r.coverage == "full")
        return any_cov / len(tier_items), full_cov / len(tier_items)

    classifications = [q.classification for q in quality_scores]

    surf_cov, surf_full = _tier_fractions(coverage_results, "surface")
    std_cov, std_full = _tier_fractions(coverage_results, "standard")
    deep_cov, deep_full = _tier_fractions(coverage_results, "deep")

    return LLMJudgeScores(
        mean_redundancy=_mean([q.redundancy for q in quality_scores]),
        mean_specificity=_mean([q.specificity for q in quality_scores]),
        mean_actionability=_mean([q.actionability for q in quality_scores]),
        mean_depth=_mean([q.depth for q in quality_scores]),
        mean_accuracy=_mean([q.accuracy for q in quality_scores]),
        high_value_count=classifications.count("high_value"),
        moderate_value_count=classifications.count("moderate_value"),
        low_value_count=classifications.count("low_value"),
        noise_count=classifications.count("noise"),
        surface_coverage=surf_cov,
        standard_coverage=std_cov,
        deep_coverage=deep_cov,
        surface_full=surf_full,
        standard_full=std_full,
        deep_full=deep_full,
        quality_scores=quality_scores,
        coverage_results=coverage_results,
        judge_model=judge_model,
    )


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def judge_run(
    run: RunResult,
    task_config,  # TaskConfig — avoid circular import
    judge_config: dict,
) -> Optional[LLMJudgeScores]:
    """Run both quality and coverage judges on a single eval run.

    Returns None if the API key is missing or all calls fail.
    """
    model = judge_config.get("model", "claude-sonnet-4-5")
    max_retries = judge_config.get("max_retries", 3)
    diff_max_chars = judge_config.get("diff_max_chars", 4000)

    try:
        judge = AnthropicJudge(model=model, max_retries=max_retries)
    except RuntimeError as e:
        print(f"  Warning: {e}", file=sys.stderr)
        return None

    # --- Quality scoring ---
    quality_scores: list[WisdomQualityScore] = []
    entry_idx = 0
    for ann in run.annotations:
        for w in ann.wisdom:
            score = _judge_quality_entry(
                judge=judge,
                entry_index=entry_idx,
                category=w.category,
                content=w.content,
                file=w.file or "",
                task_prompt=task_config.prompt,
                commit_messages=run.commit_messages,
                diff=run.diff_text,
                diff_max_chars=diff_max_chars,
            )
            if score is not None:
                quality_scores.append(score)
            entry_idx += 1
            # Brief pause to be kind to rate limits
            time.sleep(0.5)

    # --- Coverage scoring ---
    ground_truth_dicts = [
        {
            "category": gt.category,
            "content": gt.content,
            "tier": gt.tier,
        }
        for gt in task_config.ground_truth
    ]
    annotation_dicts = [asdict(ann) for ann in run.annotations]

    coverage_results = _judge_coverage(
        judge=judge,
        ground_truth=ground_truth_dicts,
        annotations=annotation_dicts,
        task_prompt=task_config.prompt,
    )

    return _aggregate_scores(quality_scores, coverage_results, model)


def judge_from_jsonl(
    jsonl_path: Path,
    judge_config: dict,
) -> Path:
    """Re-judge existing runs from a JSONL file.

    Writes judged results to a sibling file `judged-runs.jsonl` in the
    same directory. Returns the output path.
    """
    from eval.driver import load_task_config
    from eval.models import Annotation, WisdomEntry

    output_path = jsonl_path.parent / "judged-runs.jsonl"

    with open(jsonl_path) as f:
        lines = [line.strip() for line in f if line.strip()]

    with open(output_path, "w") as out:
        for i, line in enumerate(lines):
            entry = json.loads(line)
            run_data = entry["run"]
            scores_data = entry["scores"]

            task_id = run_data["task_id"]
            print(f"\nJudging run {i + 1}/{len(lines)}: {task_id}")

            # Reconstruct RunResult
            annotations = []
            for ann_data in run_data.get("annotations", []):
                wisdom = [
                    WisdomEntry(
                        category=w["category"],
                        content=w["content"],
                        file=w.get("file"),
                    )
                    for w in ann_data.get("wisdom", [])
                ]
                annotations.append(Annotation(
                    commit_sha=ann_data["commit_sha"],
                    timestamp=ann_data["timestamp"],
                    summary=ann_data["summary"],
                    wisdom=wisdom,
                ))

            run_result = RunResult(
                task_id=run_data["task_id"],
                prompt_variant=run_data["prompt_variant"],
                annotations=annotations,
                commit_messages=run_data.get("commit_messages", []),
                files_changed=run_data.get("files_changed", []),
                diff_text=run_data.get("diff_text", ""),
                agent_output=run_data.get("agent_output", ""),
                elapsed_seconds=run_data.get("elapsed_seconds", 0.0),
                success=run_data.get("success", True),
                error=run_data.get("error"),
            )

            # Load task config for ground truth
            try:
                task_config = load_task_config(task_id)
            except Exception as e:
                print(f"  Warning: could not load task config for {task_id}: {e}",
                      file=sys.stderr)
                # Write entry without judge scores
                out.write(json.dumps(entry, default=str) + "\n")
                continue

            # Judge
            judge_scores = judge_run(run_result, task_config, judge_config)

            # Merge into scores
            if judge_scores is not None:
                scores_data["judge"] = asdict(judge_scores)

            entry["scores"] = scores_data
            out.write(json.dumps(entry, default=str) + "\n")

    print(f"\nJudged results written to: {output_path}")
    return output_path
