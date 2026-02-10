"""Heuristic scoring for Chronicle annotation quality.

Six metrics that catch obviously bad annotations. These are NOT quality
measures — they filter the floor, not the ceiling. LLM-as-judge (Phase 2)
measures actual quality.
"""

from __future__ import annotations

import re

from eval.models import HeuristicScores, RunResult, ScoreReport


def _trigrams(text: str) -> set[str]:
    """Extract character trigrams from text."""
    text = text.lower().strip()
    if len(text) < 3:
        return {text} if text else set()
    return {text[i : i + 3] for i in range(len(text) - 2)}


def _jaccard(a: set, b: set) -> float:
    """Jaccard similarity between two sets."""
    if not a and not b:
        return 0.0
    intersection = a & b
    union = a | b
    return len(intersection) / len(union) if union else 0.0


def msg_overlap(summary: str, commit_messages: list[str]) -> float:
    """Trigram Jaccard similarity between annotation summary and commit messages.

    High overlap means the summary restates the commit message — low value.
    """
    if not summary or not commit_messages:
        return 0.0
    summary_trigrams = _trigrams(summary)
    msg_text = " ".join(commit_messages)
    msg_trigrams = _trigrams(msg_text)
    return _jaccard(summary_trigrams, msg_trigrams)


def specificity(wisdom_entries: list) -> float:
    """Count of concrete code references in wisdom content.

    Looks for file paths, function references, and line numbers.
    Also counts entries with a .file field set.
    """
    score = 0.0
    file_path_re = re.compile(r"[\w/]+\.\w{1,5}")
    func_ref_re = re.compile(r"\w+\(\)")
    line_ref_re = re.compile(r"line\s*\d+", re.IGNORECASE)

    for entry in wisdom_entries:
        content = entry.content
        score += len(file_path_re.findall(content))
        score += len(func_ref_re.findall(content))
        score += len(line_ref_re.findall(content))
        if entry.file:
            score += 1

    return score


def wisdom_density(wisdom_count: int, files_changed: list[str]) -> float:
    """Wisdom entries per file changed."""
    if not files_changed:
        return 0.0
    return wisdom_count / len(files_changed)


def category_coverage(categories: set[str]) -> float:
    """Fraction of the four standard categories used."""
    standard = {"dead_end", "gotcha", "insight", "unfinished_thread"}
    return len(categories & standard) / len(standard)


def grounding_ratio(wisdom_entries: list) -> float:
    """Fraction of wisdom entries that have a .file field."""
    if not wisdom_entries:
        return 0.0
    grounded = sum(1 for e in wisdom_entries if e.file)
    return grounded / len(wisdom_entries)


def content_length(wisdom_entries: list) -> float:
    """Mean word count per wisdom entry."""
    if not wisdom_entries:
        return 0.0
    word_counts = [len(e.content.split()) for e in wisdom_entries]
    return sum(word_counts) / len(word_counts)


def score_run(run: RunResult) -> ScoreReport:
    """Compute all heuristic scores for a run result."""
    all_wisdom = []
    all_categories = set()
    summaries = []

    for ann in run.annotations:
        summaries.append(ann.summary)
        for w in ann.wisdom:
            all_wisdom.append(w)
            all_categories.add(w.category)

    combined_summary = " ".join(summaries)

    scores = HeuristicScores(
        msg_overlap=msg_overlap(combined_summary, run.commit_messages),
        specificity=specificity(all_wisdom),
        wisdom_density=wisdom_density(len(all_wisdom), run.files_changed),
        category_coverage=category_coverage(all_categories),
        grounding_ratio=grounding_ratio(all_wisdom),
        content_length=content_length(all_wisdom),
    )

    return ScoreReport(
        task_id=run.task_id,
        prompt_variant=run.prompt_variant,
        heuristic=scores,
        annotation_count=len(run.annotations),
        wisdom_count=len(all_wisdom),
    )
