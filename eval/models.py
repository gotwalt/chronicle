"""Data structures for the Chronicle wisdom extraction eval framework."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional


@dataclass
class GroundTruth:
    category: str
    content: str
    tier: str  # "surface" | "standard" | "deep"
    discoverable_via: str


@dataclass
class TaskConfig:
    id: str
    name: str
    difficulty: str
    prompt: str
    init_script: str  # path to setup.sh
    ground_truth: list[GroundTruth] = field(default_factory=list)


@dataclass
class WisdomEntry:
    category: str
    content: str
    file: Optional[str] = None


@dataclass
class Annotation:
    commit_sha: str
    timestamp: str
    summary: str
    wisdom: list[WisdomEntry] = field(default_factory=list)


@dataclass
class RunResult:
    task_id: str
    prompt_variant: str
    annotations: list[Annotation]
    commit_messages: list[str]
    files_changed: list[str]
    diff_text: str
    agent_output: str
    elapsed_seconds: float
    success: bool
    error: Optional[str] = None


@dataclass
class HeuristicScores:
    msg_overlap: float
    specificity: float
    wisdom_density: float
    category_coverage: float
    grounding_ratio: float
    content_length: float


@dataclass
class WisdomQualityScore:
    entry_index: int
    category: str
    redundancy: int  # 1-5
    specificity: int  # 1-5
    actionability: int  # 1-5
    depth: int  # 1-5
    accuracy: int  # 1-5
    classification: str  # "high_value" | "moderate_value" | "low_value" | "noise"
    reasoning: str


@dataclass
class CoverageResult:
    ground_truth_index: int
    tier: str  # "surface" | "standard" | "deep"
    coverage: str  # "full" | "partial" | "missed"
    matched_entry: Optional[int]
    explanation: str


@dataclass
class LLMJudgeScores:
    mean_redundancy: float
    mean_specificity: float
    mean_actionability: float
    mean_depth: float
    mean_accuracy: float
    high_value_count: int
    moderate_value_count: int
    low_value_count: int
    noise_count: int
    surface_coverage: float  # fraction full|partial
    standard_coverage: float
    deep_coverage: float
    surface_full: float  # fraction specifically "full"
    standard_full: float
    deep_full: float
    quality_scores: list[WisdomQualityScore] = field(default_factory=list)
    coverage_results: list[CoverageResult] = field(default_factory=list)
    judge_model: str = ""


@dataclass
class ScoreReport:
    task_id: str
    prompt_variant: str
    heuristic: HeuristicScores
    annotation_count: int
    wisdom_count: int
    judge: Optional[LLMJudgeScores] = None


@dataclass
class VariantSummary:
    variant: str
    tasks_run: int
    mean_wisdom_count: float
    mean_quality: dict[str, float]  # {redundancy, specificity, actionability, depth, accuracy}
    classification_counts: dict[str, int]  # {high_value, moderate_value, low_value, noise}
    coverage_by_tier: dict[str, dict[str, int]]  # {surface: {full, partial, missed}, ...}


@dataclass
class ComparisonReport:
    baseline: VariantSummary
    experiment: VariantSummary
    coverage_delta: dict[str, float]  # {surface, standard, deep} — experiment minus baseline
