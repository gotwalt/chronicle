"""Entry point for the Chronicle eval framework.

Usage: uv run python -m eval [options]
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

from eval.driver import PROJECT_ROOT, load_eval_config, run_single
from eval.models import ScoreReport


def _resolve_judge_config(args, config: dict) -> tuple[bool, dict]:
    """Resolve whether judging is enabled and the judge config dict.

    CLI flags override config file settings.
    """
    judge_section = config.get("judge", {})
    judge_config = {
        "model": judge_section.get("model", "claude-sonnet-4-5"),
        "max_retries": judge_section.get("max_retries", 3),
        "diff_max_chars": judge_section.get("diff_max_chars", 4000),
    }

    # CLI overrides
    if args.judge_model:
        judge_config["model"] = args.judge_model

    # Determine enabled state: --judge/--no-judge override config
    if args.judge:
        enabled = True
    elif args.no_judge:
        enabled = False
    else:
        enabled = judge_section.get("enabled", False)

    return enabled, judge_config


def _print_judge_summary(reports: list[ScoreReport]):
    """Print the LLM judge summary tables for reports that have judge scores."""
    judged = [r for r in reports if r.judge is not None]
    if not judged:
        return

    print(f"\n{'='*60}")
    print("LLM JUDGE — Quality")
    print(f"{'='*60}")
    print(
        f"{'Task':<22} {'Rdnd':>5} {'Spec':>5} {'Actn':>5} "
        f"{'Dpth':>5} {'Accy':>5} {'Class':>14}"
    )
    print("-" * 67)
    for r in judged:
        j = r.judge
        cls = (
            f"{j.high_value_count}H/"
            f"{j.moderate_value_count}M/"
            f"{j.low_value_count}L/"
            f"{j.noise_count}N"
        )
        print(
            f"{r.task_id:<22} {j.mean_redundancy:>5.1f} {j.mean_specificity:>5.1f} "
            f"{j.mean_actionability:>5.1f} {j.mean_depth:>5.1f} "
            f"{j.mean_accuracy:>5.1f} {cls:>14}"
        )

    print(f"\nLLM JUDGE — Coverage")
    print(f"{'='*60}")
    print(f"{'Task':<22} {'Surf':>8} {'Stnd':>8} {'Deep':>8}")
    print("-" * 50)
    for r in judged:
        j = r.judge

        def _fmt_tier(results, tier):
            items = [c for c in results if c.tier == tier]
            if not items:
                return "-"
            full = sum(1 for c in items if c.coverage == "full")
            partial = sum(1 for c in items if c.coverage == "partial")
            total = len(items)
            return f"{full}+{partial}p/{total}"

        from eval.models import CoverageResult
        surf = _fmt_tier(j.coverage_results, "surface")
        stnd = _fmt_tier(j.coverage_results, "standard")
        deep = _fmt_tier(j.coverage_results, "deep")
        print(f"{r.task_id:<22} {surf:>8} {stnd:>8} {deep:>8}")

    print(f"\n  (Coverage: full+partial/total, model: {judged[0].judge.judge_model})")


def main():
    parser = argparse.ArgumentParser(
        description="Chronicle wisdom extraction eval",
    )
    parser.add_argument(
        "--task",
        help="Run a specific task (default: all from config)",
    )
    parser.add_argument(
        "--prompt",
        help="Prompt variant to use (default: from config)",
    )
    parser.add_argument(
        "--config",
        default=str(PROJECT_ROOT / "eval" / "config.toml"),
        help="Path to config.toml",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would run without executing",
    )
    parser.add_argument(
        "--output",
        help="Output directory (default: eval/results/<timestamp>)",
    )
    # Judge flags
    parser.add_argument(
        "--judge",
        action="store_true",
        default=False,
        help="Enable LLM judging (overrides config)",
    )
    parser.add_argument(
        "--no-judge",
        action="store_true",
        default=False,
        help="Disable LLM judging (overrides config)",
    )
    parser.add_argument(
        "--judge-only",
        metavar="RUNS_JSONL",
        help="Judge existing results without re-running agents",
    )
    parser.add_argument(
        "--judge-model",
        help="Override judge model (default: from config)",
    )
    args = parser.parse_args()

    config = load_eval_config(Path(args.config))
    judge_enabled, judge_config = _resolve_judge_config(args, config)

    # --judge-only mode: judge existing results and exit
    if args.judge_only:
        from eval.judge import judge_from_jsonl

        jsonl_path = Path(args.judge_only)
        if not jsonl_path.exists():
            print(f"Error: file not found: {jsonl_path}")
            sys.exit(1)

        print(f"Judging existing results: {jsonl_path}")
        print(f"Judge model: {judge_config['model']}")
        judge_from_jsonl(jsonl_path, judge_config)
        return

    tasks = [args.task] if args.task else config["eval"]["tasks"]
    prompt_variant = args.prompt or config["eval"]["prompt_variant"]

    # Resolve output directory
    if args.output:
        output_dir = Path(args.output)
    else:
        timestamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
        output_dir = PROJECT_ROOT / "eval" / "results" / timestamp
    output_dir.mkdir(parents=True, exist_ok=True)

    # Verify chronicle binary exists
    chronicle_binary = config["chronicle"]["binary"]
    if not os.path.isabs(chronicle_binary):
        chronicle_binary = str(PROJECT_ROOT / chronicle_binary)
    if not os.path.exists(chronicle_binary):
        print(f"Error: Chronicle binary not found at {chronicle_binary}")
        print("Run `cargo build` first.")
        sys.exit(1)

    if args.dry_run:
        print("=== DRY RUN ===")
        print(f"Tasks:   {tasks}")
        print(f"Prompt:  {prompt_variant}")
        print(f"Model:   {config['agent']['model']}")
        print(f"Budget:  ${config['agent']['max_budget_usd']:.2f}")
        print(f"Timeout: {config['agent']['timeout_seconds']}s")
        print(f"Binary:  {chronicle_binary}")
        print(f"Output:  {output_dir}")
        print(f"Judge:   {'enabled' if judge_enabled else 'disabled'}")
        if judge_enabled:
            print(f"  Model:       {judge_config['model']}")
            print(f"  Max retries: {judge_config['max_retries']}")
            print(f"  Diff chars:  {judge_config['diff_max_chars']}")
        for task_id in tasks:
            from eval.driver import load_task_config
            tc = load_task_config(task_id)
            print(f"\n  Task: {tc.id} ({tc.difficulty})")
            print(f"  Name: {tc.name}")
            print(f"  Ground truth: {len(tc.ground_truth)} items")
            for gt in tc.ground_truth:
                print(f"    [{gt.tier}] {gt.category}: {gt.content[:60]}...")
        return

    # Run each task
    runs_file = output_dir / "runs.jsonl"
    reports: list[ScoreReport] = []

    for task_id in tasks:
        print(f"\n{'='*60}")
        print(f"Running task: {task_id}")
        print(f"Prompt: {prompt_variant}")
        print(f"{'='*60}")

        run_result, score_report = run_single(
            task_id=task_id,
            prompt_variant=prompt_variant,
            config=config,
        )

        # LLM judging
        if judge_enabled and run_result.success:
            from eval.driver import load_task_config
            from eval.judge import judge_run

            print("  Running LLM judge...")
            task_config = load_task_config(task_id)
            judge_scores = judge_run(run_result, task_config, judge_config)
            score_report.judge = judge_scores
            if judge_scores:
                print(f"  Judge: {judge_scores.high_value_count}H/"
                      f"{judge_scores.moderate_value_count}M/"
                      f"{judge_scores.low_value_count}L/"
                      f"{judge_scores.noise_count}N")

        reports.append(score_report)

        # Append to runs.jsonl
        with open(runs_file, "a") as f:
            entry = {
                "run": dataclasses.asdict(run_result),
                "scores": dataclasses.asdict(score_report),
            }
            f.write(json.dumps(entry, default=str) + "\n")

        if run_result.success:
            print(f"  Success in {run_result.elapsed_seconds:.1f}s")
            print(f"  Annotations: {score_report.annotation_count}")
            print(f"  Wisdom entries: {score_report.wisdom_count}")
        else:
            print(f"  FAILED: {run_result.error}")

    # Print summary table
    print(f"\n{'='*60}")
    print("SUMMARY — Heuristics")
    print(f"{'='*60}")
    print(
        f"{'Task':<22} {'Ann':>4} {'Wis':>4} "
        f"{'Ovlp':>5} {'Spec':>5} {'Dens':>5} "
        f"{'CCov':>5} {'Grnd':>5} {'WLen':>5}"
    )
    print("-" * 76)
    for r in reports:
        h = r.heuristic
        print(
            f"{r.task_id:<22} {r.annotation_count:>4} {r.wisdom_count:>4} "
            f"{h.msg_overlap:>5.2f} {h.specificity:>5.1f} {h.wisdom_density:>5.2f} "
            f"{h.category_coverage:>5.2f} {h.grounding_ratio:>5.2f} {h.content_length:>5.1f}"
        )

    # Judge summary
    _print_judge_summary(reports)

    print(f"\nResults written to: {runs_file}")


if __name__ == "__main__":
    main()
