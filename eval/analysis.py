"""Cross-variant comparison for eval results.

Reads JSONL result files and produces structured comparison reports.
"""

from __future__ import annotations

import json
from pathlib import Path

from eval.models import ComparisonReport, VariantSummary


def load_runs(path: Path) -> list[dict]:
    """Parse a JSONL file into a list of {run, scores} entries."""
    entries = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                entries.append(json.loads(line))
    return entries


def _summarize_variant(variant: str, entries: list[dict]) -> VariantSummary:
    """Aggregate scores for a single variant across all its runs."""
    tasks_run = len(entries)

    # Wisdom counts
    wisdom_counts = [e["scores"]["wisdom_count"] for e in entries]
    mean_wisdom = sum(wisdom_counts) / len(wisdom_counts) if wisdom_counts else 0.0

    # Quality means (from judge scores, if present)
    quality_dims = ["redundancy", "specificity", "actionability", "depth", "accuracy"]
    mean_quality: dict[str, float] = {}
    judged = [e for e in entries if e["scores"].get("judge")]

    if judged:
        for dim in quality_dims:
            values = [e["scores"]["judge"][f"mean_{dim}"] for e in judged]
            mean_quality[dim] = sum(values) / len(values)
    else:
        for dim in quality_dims:
            mean_quality[dim] = 0.0

    # Classification counts (summed across all runs)
    class_counts = {"high_value": 0, "moderate_value": 0, "low_value": 0, "noise": 0}
    for e in judged:
        j = e["scores"]["judge"]
        class_counts["high_value"] += j.get("high_value_count", 0)
        class_counts["moderate_value"] += j.get("moderate_value_count", 0)
        class_counts["low_value"] += j.get("low_value_count", 0)
        class_counts["noise"] += j.get("noise_count", 0)

    # Coverage by tier
    tiers = ["surface", "standard", "deep"]
    coverage_by_tier: dict[str, dict[str, int]] = {}
    for tier in tiers:
        coverage_by_tier[tier] = {"full": 0, "partial": 0, "missed": 0}

    for e in judged:
        for cr in e["scores"]["judge"].get("coverage_results", []):
            tier = cr["tier"]
            cov = cr["coverage"]
            if tier in coverage_by_tier and cov in coverage_by_tier[tier]:
                coverage_by_tier[tier][cov] += 1

    return VariantSummary(
        variant=variant,
        tasks_run=tasks_run,
        mean_wisdom_count=mean_wisdom,
        mean_quality=mean_quality,
        classification_counts=class_counts,
        coverage_by_tier=coverage_by_tier,
    )


def compare_variants(
    baseline_path: Path, experiment_path: Path
) -> ComparisonReport:
    """Compare two result sets and produce a structured report."""
    baseline_entries = load_runs(baseline_path)
    experiment_entries = load_runs(experiment_path)

    # Determine variant names from the data
    baseline_variant = _detect_variant(baseline_entries, "baseline")
    experiment_variant = _detect_variant(experiment_entries, "experiment")

    baseline_summary = _summarize_variant(baseline_variant, baseline_entries)
    experiment_summary = _summarize_variant(experiment_variant, experiment_entries)

    # Coverage delta: experiment fraction minus baseline fraction per tier
    tiers = ["surface", "standard", "deep"]
    coverage_delta: dict[str, float] = {}
    for tier in tiers:
        b = baseline_summary.coverage_by_tier.get(tier, {})
        e = experiment_summary.coverage_by_tier.get(tier, {})
        b_total = sum(b.values())
        e_total = sum(e.values())
        b_frac = (b.get("full", 0) + b.get("partial", 0)) / b_total if b_total else 0.0
        e_frac = (e.get("full", 0) + e.get("partial", 0)) / e_total if e_total else 0.0
        coverage_delta[tier] = e_frac - b_frac

    return ComparisonReport(
        baseline=baseline_summary,
        experiment=experiment_summary,
        coverage_delta=coverage_delta,
    )


def _detect_variant(entries: list[dict], fallback: str) -> str:
    """Detect the prompt variant from entries, or use fallback."""
    variants = set()
    for e in entries:
        v = e.get("scores", {}).get("prompt_variant") or e.get("run", {}).get("prompt_variant")
        if v:
            variants.add(v)
    if len(variants) == 1:
        return variants.pop()
    return fallback


def print_comparison(report: ComparisonReport) -> None:
    """Print a formatted comparison report."""
    b = report.baseline
    e = report.experiment

    print(f"\nVARIANT COMPARISON: {b.variant} vs {e.variant}")
    print("=" * 68)

    # Header
    bname = b.variant[:20]
    ename = e.variant[:20]
    print(f"{'':>26} {bname:>20} {ename:>20} {'Delta':>8}")
    print("-" * 68)

    # Wisdom entries
    delta_w = e.mean_wisdom_count - b.mean_wisdom_count
    print(
        f"{'Wisdom entries (mean)':>26} {b.mean_wisdom_count:>20.1f} "
        f"{e.mean_wisdom_count:>20.1f} {delta_w:>+8.1f}"
    )

    # Quality dimensions
    for dim in ["redundancy", "specificity", "actionability", "depth", "accuracy"]:
        bv = b.mean_quality.get(dim, 0.0)
        ev = e.mean_quality.get(dim, 0.0)
        delta = ev - bv
        label = f"Quality — {dim.capitalize()}"
        print(f"{label:>26} {bv:>20.1f} {ev:>20.1f} {delta:>+8.1f}")

    # Classification
    def _fmt_class(c: dict[str, int]) -> str:
        return f"{c.get('high_value', 0)}H/{c.get('moderate_value', 0)}M/{c.get('low_value', 0)}L/{c.get('noise', 0)}N"

    print(f"{'Classification':>26} {_fmt_class(b.classification_counts):>20} {_fmt_class(e.classification_counts):>20}")

    # Coverage by tier
    print(f"\n{'Coverage by tier:':>26}")
    tiers = ["surface", "standard", "deep"]
    for tier in tiers:
        bc = b.coverage_by_tier.get(tier, {})
        ec = e.coverage_by_tier.get(tier, {})
        b_total = sum(bc.values())
        e_total = sum(ec.values())
        b_covered = bc.get("full", 0) + bc.get("partial", 0)
        e_covered = ec.get("full", 0) + ec.get("partial", 0)
        delta_pct = report.coverage_delta.get(tier, 0.0) * 100

        b_str = f"{b_covered}/{b_total}" if b_total else "-"
        e_str = f"{e_covered}/{e_total}" if e_total else "-"
        delta_str = f"{delta_pct:+.0f}%" if (b_total or e_total) else "-"

        print(f"  {tier.capitalize():>24} {b_str:>20} {e_str:>20} {delta_str:>8}")

    # Per-task detail
    print(f"\n{'Per-task detail:':>26}")
    _print_per_task_detail(report)


def _print_per_task_detail(report: ComparisonReport) -> None:
    """Print per-task coverage changes between baseline and experiment."""
    # We need to go back to the raw data to get per-task breakdowns.
    # Since we only have summaries, we reconstruct from coverage_results
    # by checking if we stored per-task info. For now, we note this
    # requires the raw entries. This function works with the summary only
    # by showing aggregate info.
    #
    # To get real per-task detail, the caller should use load_runs() directly.
    b = report.baseline
    e = report.experiment
    tiers = ["surface", "standard", "deep"]

    # Show note that per-task detail requires --compare with full JSONL
    print(f"  {'(aggregate across all tasks)':>24}")
    for tier in tiers:
        bc = b.coverage_by_tier.get(tier, {})
        ec = e.coverage_by_tier.get(tier, {})
        b_full = bc.get("full", 0)
        e_full = ec.get("full", 0)
        b_partial = bc.get("partial", 0)
        e_partial = ec.get("partial", 0)
        print(
            f"  {tier.capitalize():>24}  "
            f"full: {b_full}->{e_full}  partial: {b_partial}->{e_partial}"
        )


def print_per_task_comparison(
    baseline_path: Path, experiment_path: Path
) -> None:
    """Print per-task detail by loading raw JSONL entries."""
    baseline_entries = load_runs(baseline_path)
    experiment_entries = load_runs(experiment_path)

    # Index by task_id
    b_by_task: dict[str, dict] = {}
    for entry in baseline_entries:
        tid = entry["scores"]["task_id"]
        b_by_task[tid] = entry

    e_by_task: dict[str, dict] = {}
    for entry in experiment_entries:
        tid = entry["scores"]["task_id"]
        e_by_task[tid] = entry

    all_tasks = sorted(set(b_by_task) | set(e_by_task))
    tiers = ["surface", "standard", "deep"]
    tier_abbr = {"surface": "Surf", "standard": "Stnd", "deep": "Deep"}

    for task_id in all_tasks:
        parts = []
        for tier in tiers:
            b_covered = _count_covered(b_by_task.get(task_id), tier)
            e_covered = _count_covered(e_by_task.get(task_id), tier)
            parts.append(f"{tier_abbr[tier]}: {b_covered}->{e_covered}")
        print(f"  {task_id:<22} {'  '.join(parts)}")


def _count_covered(entry: dict | None, tier: str) -> int:
    """Count full+partial coverage items for a tier in a single run entry."""
    if not entry:
        return 0
    judge = entry.get("scores", {}).get("judge")
    if not judge:
        return 0
    count = 0
    for cr in judge.get("coverage_results", []):
        if cr["tier"] == tier and cr["coverage"] in ("full", "partial"):
            count += 1
    return count
