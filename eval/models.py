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
class ScoreReport:
    task_id: str
    prompt_variant: str
    heuristic: HeuristicScores
    annotation_count: int
    wisdom_count: int
