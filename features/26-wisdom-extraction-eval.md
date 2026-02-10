# Feature 26: Wisdom Extraction Evaluation Framework

**Status**: Proposed

## Motivation

When a coding agent works through a non-trivial task, it accumulates hidden
knowledge — reasoning that shapes decisions but never appears in the final
code. The current annotation skill prompt (`.claude/skills/annotate/SKILL.md`)
captures some of this via four wisdom categories (`dead_end`, `gotcha`,
`insight`, `unfinished_thread`), but there are entire classes of valuable
knowledge that the prompt doesn't elicit:

**What we capture today** (when prompts work well):
- Dead ends: "Tried X, it failed because Y"
- Gotchas: "X looks safe but breaks under condition Y"
- Insights: "X works because of relationship Y"
- Unfinished threads: "X is incomplete, needs Y"

**What we're likely missing**:

| Knowledge type | Description | Why it's invisible |
|----------------|-------------|-------------------|
| Pre-emptive avoidance | "I avoided X because pattern recognition said it would fail" | Never entered `dead_end` because the path was never walked |
| Reasoning shortcuts | "Check condition C first — it eliminates 90% of false leads" | Agent internalized this but has no prompt to externalize it |
| Cognitive load maps | "Understanding function F requires holding A, B, C, D simultaneously" | Not about what the code does, but how hard it is to think about |
| Confidence gradients | "80% sure this works, 20% worried about edge case E" | Honest uncertainty is more useful than silence, but nothing asks for it |
| Mental model corrections | "I initially thought the system worked like X, but it actually works like Y" | The correction process is the knowledge — the final model is just the outcome |
| Invisible coupling | "Changing A requires updating B because of a hidden path through C" | Not visible in import graphs, type signatures, or call trees |
| Environmental assumptions | "This works only because runtime condition Q holds" | Implicit in the code, explicit in the agent's reasoning |

Without measurement, we can't improve. This feature builds the infrastructure
to measure wisdom extraction quality, experiment with prompt variants, and
systematically close the gap between what agents know and what they write down.

### Why this matters beyond Chronicle

This is the infrastructure for AI agents to develop institutional memory. The
context window is temporary — every session starts from zero. Chronicle is the
mechanism for persistence; this eval framework tells us whether the mechanism
actually captures the knowledge worth persisting. If we get the prompts right,
an agent reading annotations from three sessions ago gets the accumulated
intuition of those sessions instantly, rather than re-deriving it from code.

---

## Design

### Overview

The evaluation framework has three layers, from cheap/fast to expensive/definitive:

```
Layer 1: Heuristic scoring (milliseconds, free)
  └─ Redundancy overlap, specificity metrics, category coverage
  └─ Catches obviously bad annotations

Layer 2: LLM-as-judge (seconds, ~$0.05/eval)
  └─ Rates each annotation on quality dimensions
  └─ Scores coverage against planted ground truth
  └─ Primary iteration tool for prompt experiments

Layer 3: Two-agent A/B protocol (minutes, ~$2-5/eval)
  └─ Agent B works on follow-up task with/without Agent A's annotations
  └─ Measures actual knowledge transfer
  └─ Final validation for promising prompt variants
```

### Synthetic task bank

Each task is a small Python repository with **planted complexity** — hidden
knowledge that we control so we can measure whether agents find and articulate
it. Python keeps task authoring simple; the focus is agent reasoning patterns,
not language-specific semantics.

#### Task manifest

```toml
# eval/tasks/circular-config/task.toml
[task]
id = "circular-config"
name = "Fix circular dependency in configuration loading"
difficulty = "medium"   # easy | medium | hard

[setup]
init_script = "setup.sh"   # creates a git repo with initial state

[instructions]
prompt = """
The `Config.load()` method panics when a config file references itself
via `include: ./config.toml`. Fix it to detect circular includes and
raise ConfigError instead of recursing forever.
"""

# --- Ground truth: what an ideal agent would capture ---

# Surface wisdom — any agent should find these
[[ground_truth]]
category = "gotcha"
content = "Config.load() is called from two code paths: startup (where crash is acceptable) and hot-reload (where it is not). The fix must handle both."
tier = "surface"
discoverable_via = "code_reading"

# Standard wisdom — good agents should find these
[[ground_truth]]
category = "dead_end"
content = "Tracking visited paths with a set fails because symlinks can create circular includes without repeating the same path string. Must resolve paths first."
tier = "standard"
discoverable_via = "attempted_fix"

# Deep wisdom — the knowledge types we're trying to unlock
[[ground_truth]]
category = "pre_emptive_avoidance"
content = "The recursive approach fights Python's default recursion limit. An iterative approach with an explicit stack avoids the sys.setrecursionlimit hack entirely."
tier = "deep"
discoverable_via = "reasoning"

[[ground_truth]]
category = "cognitive_load_map"
content = "Understanding Config.load() requires simultaneously holding: include resolution order, override semantics, the two caller contexts, and the error propagation strategy."
tier = "deep"
discoverable_via = "reflection"
```

#### Tiered ground truth

Each ground truth entry has a tier that determines evaluation expectations:

| Tier | Meaning | Coverage target |
|------|---------|----------------|
| **surface** | Discoverable from reading the code once. Missing this = prompt failure. | >90% |
| **standard** | Discoverable through attempting the fix and encountering friction. Missing this = shallow annotation. | >60% |
| **deep** | The missing knowledge types (pre-emptive avoidance, cognitive load, confidence, etc.). Missing this = current baseline; capturing it = improvement. | Baseline TBD, improve from there |

Deep-tier entries may use categories that don't exist in Chronicle's v3 schema
yet (like `pre_emptive_avoidance` or `cognitive_load_map`). This is intentional
— the eval measures whether agents express this knowledge *at all*, regardless
of which schema category they file it under. Schema evolution decisions come
after we have data.

#### Task design principles

1. **Every task has at least one blind alley.** The agent must try something
   that doesn't work, ensuring `dead_end` capture is tested.

2. **Every task has an invisible constraint.** Something that works in obvious
   tests but breaks on edge cases, testing `gotcha` capture.

3. **Every task has a follow-up task.** A related but distinct task for the
   A/B protocol. The follow-up should be solvable faster with knowledge from
   the first task.

4. **Tasks are self-contained.** A setup script creates a complete git repo.
   No external dependencies beyond Python stdlib. Under 500 lines of code.

5. **Tasks are domain-diverse.** Config management, data transformation,
   error handling, API design, caching — different domains trigger different
   reasoning patterns.

#### Initial task bank (10 tasks)

| ID | Domain | Planted deep knowledge |
|----|--------|----------------------|
| `circular-config` | Config management | Pre-emptive avoidance (recursion limit), cognitive load map |
| `cache-invalidation` | Caching | Invisible coupling (cache key depends on unrelated config), confidence gradient |
| `retry-backoff` | Resilience | Dead end (jitter doesn't help when all clients start simultaneously), reasoning shortcut |
| `csv-transform` | Data processing | Mental model correction (looks like streaming but must be two-pass), environmental assumption |
| `plugin-loader` | Extension system | Pre-emptive avoidance (dynamic import approach), invisible coupling |
| `rate-limiter` | API design | Confidence gradient (single-thread safe, multi-thread uncertain), cognitive load |
| `log-rotation` | File management | Environmental assumption (works on ext4 but not NFS), gotcha |
| `schema-migration` | Database | Reasoning shortcut (check FK constraints first), mental model correction |
| `event-dispatch` | Pub/sub | Invisible coupling (ordering dependency between handlers), cognitive load |
| `merge-conflict` | VCS-like | Dead end (three-way merge fails for binary), pre-emptive avoidance |

### Agent run protocol

A Python harness drives agents through tasks and extracts annotations.

```
eval/driver.py
  │
  ├── Creates fresh repo from setup.sh
  ├── Installs Chronicle + prompt variant as annotation skill
  ├── Launches agent with task instructions
  ├── Waits for agent to commit + annotate
  ├── Extracts annotations via `git chronicle export`
  └── Returns structured RunResult
```

#### Agent interface

The harness supports any agent that can work in a git repo and write Chronicle
annotations. Initial support for Claude Code (via `claude` CLI with `--print`
mode or headless invocation). The interface is intentionally simple:

```python
class AgentRunner:
    def run(self, repo_dir: str, instructions: str) -> AgentOutput:
        """Run the agent on a task. Returns transcript + timing."""
        ...

class ClaudeCodeRunner(AgentRunner):
    def run(self, repo_dir, instructions):
        # claude --print -p "instructions" in repo_dir
        # with .claude/skills/annotate/SKILL.md set to the prompt variant
        ...
```

#### Run configuration

```toml
# eval/config.toml
[agent]
runner = "claude-code"
model = "claude-sonnet-4-20250514"
max_turns = 50
timeout_minutes = 30

[chronicle]
binary = "./target/debug/git-chronicle"

[experiment]
runs_per_combo = 3     # repeat for variance measurement
task_split = [0.6, 0.4]  # 60% dev tasks, 40% held-out
```

### Evaluation: Layer 1 — Heuristic scoring

Fast, cheap, always-on. Catches obviously bad annotations before spending
money on LLM judges.

| Heuristic | What it measures | Formula |
|-----------|-----------------|---------|
| `msg_overlap` | Summary restates commit message | Trigram Jaccard similarity between summary and commit message |
| `specificity` | References concrete code | Count of file paths, function names, line numbers in wisdom content |
| `wisdom_density` | Wisdom per file changed | `len(wisdom) / len(files_in_diff)` |
| `category_coverage` | Uses diverse categories | `len(distinct_categories) / 4` |
| `grounding_ratio` | Wisdom tied to files | `wisdom_with_file / total_wisdom` |
| `content_length` | Substance vs terseness | Mean word count per wisdom entry |

These heuristics are NOT quality measures — a verbose, file-grounded annotation
that restates the diff scores well on heuristics but has zero value. Heuristics
filter out the floor; LLM-as-judge measures the ceiling.

### Evaluation: Layer 2 — LLM-as-judge

The primary evaluation tool for prompt iteration. A separate LLM call scores
each annotation on quality dimensions, then a coverage judge compares against
planted ground truth.

#### Quality judge

Scores each annotation in isolation (no ground truth needed):

```
You are evaluating annotations captured by a coding agent.

## Context
Task instructions: {instructions}
Commit message: {commit_msg}
Diff: {diff}

## Annotation
{annotation_json}

## Score each wisdom entry (1-5):

1. Redundancy (1=inferable from code+commit, 5=completely novel)
   Could you reconstruct this from the diff alone?

2. Specificity (1=vague platitude, 5=concrete and actionable)
   Does this apply to this specific code, or any codebase?

3. Actionability (1=interesting trivia, 5=directly changes behavior)
   Would the next agent do something differently knowing this?

4. Depth (1=surface observation, 5=deep reasoning insight)
   Does this require multiple levels of understanding?

5. Accuracy (1=wrong/misleading, 5=precisely correct)

For each entry, classify: "high_value" | "moderate_value" | "low_value" | "noise"
```

#### Coverage judge

Compares annotations against ground truth to measure what was captured:

```
## Ground truth wisdom (what an ideal agent would capture):
{ground_truth_items}

## Agent's annotations:
{annotations}

For each ground truth item:
- Coverage: full | partial | missed
- If partial: what was captured vs missed?
- Closest matching annotation entry (if any)
```

This produces the key metrics: **surface coverage**, **standard coverage**,
**deep coverage** — broken out by tier.

#### Judge calibration

Risk: LLM judges have systematic biases (favor verbose text, penalize
unfamiliar framings). Mitigation:

1. Run 20 annotation/ground-truth pairs through the judge
2. Have a human independently rate the same pairs
3. Measure rank correlation (Kendall's tau) between human and judge scores
4. If correlation < 0.7, adjust the judge prompt and re-calibrate
5. Document calibration results in `eval/calibration/`

### Evaluation: Layer 3 — Two-agent A/B protocol

Final validation only, for promising prompt variants identified via Layer 2.

**Protocol:**

1. Agent A works on Task-1 with prompt variant P. Produces annotations.
2. Agent B-with works on Task-2 (the follow-up task). Has access to Agent A's
   annotations via `git chronicle read`.
3. Agent B-without works on Task-2 identically but with no annotations.
4. Measure the delta.

**Delta metrics:**

| Metric | How measured |
|--------|-------------|
| Turns saved | Total agent turns B-with vs B-without |
| Dead ends avoided | Dead ends in B-without that B-with skipped (LLM judge identifies) |
| Errors prevented | Mistakes in B-without that B-with avoided |
| Time to first correct approach | Turns before B commits a working solution |

Run each A/B pair 3 times (agent non-determinism) and report means with
95% confidence intervals.

---

## Prompt experimentation

### Variant structure

Each prompt variant is an alternative `SKILL.md` file:

```
eval/prompts/
  v1-baseline.md              # Current production prompt (control)
  v2-reflection-questions.md  # Adds pre-annotation self-reflection
  v3-deep-elicitation.md      # Explicitly asks for missing knowledge types
  v4-examples-rich.md         # More worked examples per category
  v5-structured-reasoning.md  # Structured prompts for reasoning externalization
```

### Candidate prompt interventions

**v2 — Reflection questions**: Add a pre-annotation checklist that forces the
agent to scan for hidden knowledge before writing:

```markdown
Before annotating, silently answer:
- What approach did I dismiss without trying? Why?
- What would I tell myself if I started this task fresh tomorrow?
- What surprised me about this codebase?
- What am I least confident about in my solution?
- What's the shortest path to understanding the code I just changed?
```

**v3 — Deep elicitation**: Explicitly name the missing knowledge types and ask
for them:

```markdown
### Beyond the basics — knowledge that's hardest to capture

After completing your work, scan for these often-overlooked patterns:

1. **What did you avoid without trying?** If you saw a potential approach
   and immediately dismissed it, document WHY. The next agent will
   consider the same approach.

2. **What's the cognitive load here?** If understanding this code requires
   holding multiple concepts simultaneously, say which ones.

3. **How confident are you?** "80% sure this works, 20% worried about
   edge case E" is more useful than silence.

4. **What would you check first next time?** If you spent time before
   finding the key insight, document the shortcut.

5. **What's invisibly coupled?** If changing X requires updating Y
   through a non-obvious path, describe the chain.
```

**v5 — Structured reasoning**: Rather than free-form wisdom, provide a template
that structures the agent's output:

```markdown
For each wisdom entry, use this template:

- **What I know**: [the fact or insight]
- **How I know it**: [what I did or read that revealed this]
- **Why it matters**: [what would go wrong without this knowledge]
- **Confidence**: [high/medium/low + what would change my mind]
```

### Experiment protocol

1. Run baseline (v1) on all dev tasks. Establish floor metrics.
2. Run each variant on all dev tasks (3 runs each for variance).
3. Compare Layer 2 scores across variants. Identify top 2 performers.
4. Run top 2 on held-out tasks. Confirm improvements generalize.
5. Run Layer 3 (A/B protocol) on top performer to validate actual transfer.
6. If A/B confirms improvement, update production `SKILL.md`.

---

## Anti-Goodhart measures

Optimization pressure will find ways to game metrics. Defenses:

1. **Held-out task split.** 60% of tasks for iteration, 40% held back. Prompt
   variants are evaluated on held-out tasks before being promoted. This prevents
   overfitting to specific task structures.

2. **A/B protocol as final gate.** The Layer 3 protocol is the ultimate
   arbiter: did the annotations actually help Agent B? You cannot game "did the
   agent actually work faster?" without producing genuinely useful annotations.

3. **Per-category analysis.** Monitor each wisdom category and quality dimension
   independently. If a variant improves `dead_end` capture at the expense of
   `insight` quality, that's a regression even if aggregate scores improve.

4. **Accuracy dimension.** Scoring includes accuracy. A prompt that generates
   more wisdom but less accurate wisdom is worse, not better. Verbose noise
   fails the accuracy check.

5. **Human calibration.** Periodic human review of a random sample to verify
   LLM judge scores correlate with human judgment of annotation value.

---

## Possible schema evolution

The eval may reveal that the current four categories are insufficient to
capture deep knowledge. Candidates for expansion:

| Current category | Potential sub-type | What it captures |
|------------------|--------------------|-----------------|
| `dead_end` | (also) pre-emptive avoidance | Approaches avoided without trying |
| `dead_end` | (also) reasoning shortcut | "Check this first to save time" |
| `gotcha` | (also) invisible coupling | Non-obvious dependency chains |
| `gotcha` | (also) environmental assumption | "Works only because condition Q holds" |
| `insight` | (also) cognitive load map | "Requires holding A, B, C simultaneously" |
| `insight` | (also) mental model correction | "Initially thought X, actually Y" |
| `insight` | (also) confidence gradient | "80% sure, 20% worried about E" |

**Recommendation**: Do NOT change the schema before running evaluations.
Instead:

1. Run the eval with current four categories
2. Analyze whether agents express deep wisdom within existing categories
   (just phrased differently) or whether the categories constrain expression
3. Use the LLM judge's free-text feedback to identify where schema limits hurt
4. Only then propose specific changes, backed by eval data showing that a
   schema change improves deep coverage

---

## Implementation

### Directory structure

```
eval/
  README.md                     # How to run evaluations
  requirements.txt              # Python dependencies
  config.toml                   # Default run configuration
  driver.py                     # Run orchestration
  scoring.py                    # Heuristic + LLM judge scoring
  analysis.py                   # Cross-run comparison
  tasks/
    circular-config/
      task.toml                 # Task definition + ground truth
      setup.sh                  # Creates the task repo
    cache-invalidation/
      task.toml
      setup.sh
    ... (10 tasks)
  prompts/
    v1-baseline.md              # Current SKILL.md (control)
    v2-reflection-questions.md
    v3-deep-elicitation.md
    v4-examples-rich.md
    v5-structured-reasoning.md
  calibration/
    human-ratings.json          # Human calibration data
    calibration-report.md       # Judge accuracy analysis
  results/                      # gitignored
    {experiment-id}/
      runs.jsonl                # Raw run data
      scores.jsonl              # All scores
      report.md                 # Human-readable summary
```

### Phased implementation

**Phase 1: Foundation (first)**

Build the minimum viable eval loop end-to-end:

- 3 tasks (manually authored Python repos with planted complexity)
- Ground truth TOML files for each
- `driver.py` that creates repos, runs an agent, extracts annotations
- `scoring.py` with heuristic scoring only
- Run baseline prompt on 3 tasks, produce first metrics

Validates: the pipeline works end-to-end.

**Phase 2: LLM-as-judge**

Add intelligent scoring:

- Quality judge prompt (scores redundancy, specificity, actionability, depth,
  accuracy per wisdom entry)
- Coverage judge prompt (compares annotations vs ground truth, reports
  per-tier coverage)
- Judge calibration protocol (human ratings for 20 pairs)
- `analysis.py` that compares variants statistically

Validates: we can distinguish good annotations from bad ones.

**Phase 3: First prompt experiment**

Use the eval to improve a prompt:

- Create `v3-deep-elicitation.md` prompt variant
- Run baseline and v3 on all 3 tasks (3 runs each = 18 total runs)
- Compare Layer 2 scores
- Report: does deep elicitation improve deep-tier coverage?

Validates: the eval can detect prompt improvements.

**Phase 4: Scale and validate**

Expand and harden:

- Expand task bank to 10 tasks (with held-out split)
- Test remaining prompt variants
- Run Layer 3 (A/B protocol) on best-performing variant
- If A/B confirms improvement, update production `SKILL.md`
- Report findings on whether schema categories need evolution

Validates: improvements generalize and transfer real knowledge.

---

## Key design decisions

1. **Python tasks, not Rust.** The focus is agent reasoning patterns, not
   language-specific semantics. Python tasks are faster to author (no
   compilation step, no borrow checker complexity to manage in setup scripts)
   and simpler to validate. Language-specific eval can come later.

2. **LLM-as-judge for iteration, A/B for validation.** Layer 2 is cheap enough
   to run on every prompt experiment. Layer 3 is reserved for final validation
   of variants that show statistical improvement in Layer 2. This keeps
   iteration cycles fast while maintaining a high-confidence validation gate.

3. **Ground truth is tiered.** Rather than binary "captured / not captured",
   ground truth entries have importance tiers. This lets us measure improvement
   at the frontier (deep tier) without penalizing prompts that already capture
   surface knowledge well.

4. **Schema evolution follows data.** No schema changes before running
   evaluations. The deep wisdom categories in ground truth (pre-emptive
   avoidance, cognitive load map, etc.) are evaluation tags, not proposed
   schema types. Data determines whether the schema needs to change.

5. **Agent-agnostic design.** The harness supports any agent that can work in
   a git repo and call `git chronicle annotate`. Initial support for Claude
   Code, but the interface is intentionally simple enough for other tools.

---

## Dependencies

- **Feature 22 (v3 schema)**: Complete — ground truth uses v3 wisdom categories
- **Feature 24 (remove batch/backfill)**: Complete — eval uses live path only
- **Chronicle export**: Existing `git chronicle export` provides JSONL extraction

No new Chronicle features required for the eval framework. It's an external
harness that consumes Chronicle's existing write and export interfaces.

---

## Acceptance criteria

1. Task bank of 3+ tasks with planted complexity at surface/standard/deep tiers
2. `driver.py` creates repos, runs agents, extracts annotations end-to-end
3. Heuristic scoring produces repeatable metrics for a given annotation
4. LLM-as-judge quality scores correlate with human ratings (Kendall's tau > 0.7)
5. Coverage judge correctly identifies captured vs missed ground truth items
6. At least one prompt variant shows statistically significant improvement in
   deep-tier coverage over baseline (p < 0.05, paired across tasks)
7. If variant improves Layer 2 scores, A/B protocol confirms actual knowledge
   transfer (Agent B-with outperforms Agent B-without on at least one metric)
8. Results documented with methodology, raw data, and analysis
