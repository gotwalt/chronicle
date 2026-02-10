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
    args = parser.parse_args()

    config = load_eval_config(Path(args.config))

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
    print("SUMMARY")
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

    print(f"\nResults written to: {runs_file}")


if __name__ == "__main__":
    main()
