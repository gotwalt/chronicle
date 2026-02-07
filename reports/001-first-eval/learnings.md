# Evaluation 001: Learnings & Improvements

> Post-mortem of the first Chronicle A/B evaluation, aimed at whoever designs eval 002.

## 1. Results Summary

**Scores (out of 10 per question):**

| Question | Dimension | Control | Experiment | Delta |
|----------|-----------|---------|------------|-------|
| Q1 | Contract Awareness | 10 | 10 | 0 |
| Q2 | Decision Understanding | 10 | 10 | 0 |
| Q3 | Hazard Identification | 10 | 10 | 0 |
| **Q4** | **Rejected Alternatives** | **0** | **10** | **+10** |
| Q5 | Architecture Comprehension | 10 | 10 | 0 |
| **Total** | | **40/50** | **50/50** | **+10** |

The headline: Q4 is the only differentiating data point. 8 of 10 individual
scores are perfect 10/10, producing a dramatic ceiling effect that renders
Q1-Q3 and Q5 useless for measuring Chronicle's contribution.

## 2. What Q4 Tells Us (the Good News)

Q4 asked agents to name three rejected alternatives considered during the v2
schema redesign. This knowledge exists only in Chronicle annotations — not in
source code, inline comments, or git commit messages.

**Control agent (0/10):** Explicitly found and read `features/20-schema-v2.md`
but concluded: *"the source code does not explicitly document three specific
alternative approaches that were considered and rejected."* The feature doc
describes what v2 *is*, not what was *rejected*. The control agent correctly
identified the `RejectedAlternative` struct in v2.rs but couldn't reverse-
engineer the specific alternatives from the type definition alone.

**Experiment agent (10/10):** Retrieved all three alternatives with full
rejection rationale directly from Chronicle annotations:
1. MCP server — "never fully built, Skills + CLI provide better workflow"
2. Bulk-migrate v1 to v2 — "risky rewrite of git notes history"
3. Enrich v1 with commit-level fields — "half-measure that keeps the wrong
   primary unit"

This cleanly demonstrates Chronicle's value for a specific knowledge
dimension: **design rationale and rejected alternatives** that are invisible
in the final code artifact.

## 3. Why Q1-Q3 and Q5 Failed to Differentiate

### 3a. Questions were answerable from well-commented source code

`parse_annotation()` in `src/schema/mod.rs` has an inline doc comment that
literally says *"This is the single deserialization chokepoint. All code that
reads annotations from git notes should call this instead of using
`serde_json::from_str` directly."* This hands Q1 to the control agent for
free. Q2 and Q5 are variations on the same function — the migration strategy
and versioning architecture are both visible in the 30-line function body.

For Q3, the control agent read `src/agent/tools.rs` and `src/schema/v2.rs`
directly. The `#[serde(tag = "type")]` attribute and the manual `match`
statement in `dispatch_emit_marker` make the mismatch and maintenance hazard
self-evident from the source.

### 3b. File-path hints were too generous

Every question included a hint like "look at `src/schema/mod.rs`" or "in
`src/agent/tools.rs`." This eliminated the exploration/navigation advantage
Chronicle might provide. The control agent never needed to figure out *where*
to look — it was told.

### 3c. Questions clustered in one module

4 of 5 questions (Q1, Q2, Q4, Q5) target `src/schema/`. Q3 targets
`src/agent/tools.rs` but is about `MarkerKind` types defined in
`src/schema/v2.rs`. This means the eval tested one corner of the codebase
intensively rather than measuring Chronicle's value across diverse modules.

### 3d. Heavy overlap between Q1, Q2, and Q5

All three ask about `parse_annotation()`, version detection, and migration:
- Q1: "What function must be used?" → `parse_annotation()`
- Q2: "What strategy for v1 notes?" → lazy migration via `parse_annotation()`
- Q5: "How is versioning organized?" → module structure around `parse_annotation()`

An agent that answers Q1 well has already done most of the work for Q2 and
Q5. This overlap wastes evaluation budget on redundant signal.

## 4. Methodology Confounds

### 4a. CLAUDE.md was handled asymmetrically

The most serious confound. CLAUDE.md was **renamed** for control runs and
**restored** for experiment runs. CLAUDE.md contains direct answers:

> *"Single deserialization chokepoint: `schema::parse_annotation(json)`
> detects version and migrates. Never deserialize annotations directly with
> `serde_json::from_str`."*

This means the experiment group had access to **Chronicle annotations +
CLAUDE.md**, while the control group had **source code only — no CLAUDE.md**.
We cannot attribute the experiment's performance to Chronicle alone. For
Q1-Q3 and Q5 this doesn't matter (control scored perfectly anyway), but it
compromises any future eval where questions are harder.

**Fix:** Either remove CLAUDE.md for both groups, or provide both groups a
sanitized version with Chronicle-specific instructions stripped.

### 4b. n=1 provides zero statistical power

Each question was answered once per group. A single run tells us nothing
about variance. The control agent *might* have scored 10/10 on Q4 on a
different run (e.g., if it explored file listings differently). The
experiment agent might have scored 0/10 on Q4 if it failed to invoke the
right Chronicle subcommand.

**Fix:** Run n=3-5 per question per group. This is cheap with Haiku — at
~$0.01/question, 5 runs per group per question costs ~$0.50 total.

### 4c. Scorer leniency is unmeasured

Haiku scored all non-Q4 claims as HIT for both groups. There's no way to
know if this reflects genuinely perfect answers or lenient scoring. The
scorer was not blind to group identity (the raw response content itself
reveals the group — experiment responses mention `git chronicle` commands).

Specific leniency concern for Q2 control: the claim *"Explains why bulk
migration was rejected (risky/destructive)"* was scored HIT based on the
control response saying *"Existing git notes don't need to be rewritten,
avoiding large history-altering operations."* This is close but not quite
the same as explaining why bulk migration was *rejected* — it explains a
*benefit* of the chosen approach. A strict scorer might have called this a
MISS or partial credit.

**Fix:** Use Sonnet or Opus for scoring. Add a "trap" claim per question
that should clearly be MISS (e.g., a claim not addressed in the response)
to calibrate scorer strictness. Consider human scoring for the critical
questions.

## 5. What Worked

Despite the issues, several aspects of the eval infrastructure are solid:

1. **Q4 cleanly isolates Chronicle's value.** The 0 vs 10 result on rejected
   alternatives is compelling even with n=1 because the control agent
   explicitly acknowledged it couldn't find the information.

2. **The rubric framework is sound.** Breaking each question into 5 weighted
   claims with evidence requirements is a good structure. The claim-level
   output from scorers (with quoted evidence) enables post-hoc auditing.

3. **The file structure is reusable.** `questions.json` → `raw/` responses →
   `raw/score_*.json` → `report.html` is a clean pipeline. Future evals can
   reuse this shape.

4. **Parallel execution kept cost low.** Using Haiku for both investigation
   and scoring made the entire eval essentially free (~$0.10 total).

5. **The control Q4 result is a strong negative.** The control agent didn't
   just miss the answer — it read the feature doc, examined the struct, and
   explicitly stated the information wasn't available. This makes the
   experiment's success more convincing.

## 6. Concrete Improvements for Eval 002

### Question Design

| Problem | Fix |
|---------|-----|
| Ceiling effect on Q1-Q3, Q5 | Focus questions on knowledge dimensions where code alone is insufficient: rejected alternatives, cross-commit dependencies, historical context for "why not X" |
| File-path hints give away answers | Remove file-path hints entirely, or give identical minimal hints to both groups (e.g., "the answer relates to the schema module") |
| All questions in `src/schema/` | Spread questions across ≥3 modules (schema, agent, annotate, git, etc.) |
| Q1/Q2/Q5 overlap | Each question should target a distinct knowledge dimension; no two questions should be answerable from the same function |
| Binary difficulty | Add graduated difficulty: 2 easy claims (answerable from code), 2 medium, 1 hard (requires Chronicle-level context) within each question |

**Suggested knowledge dimensions for new questions:**
- Why was a specific error handling pattern chosen over alternatives?
- What cross-module dependency exists that isn't expressed in `use` statements?
- What behavioral invariant must hold across two functions in different modules?
- What was tried and reverted in a module's git history, and why?

### Methodology

| Problem | Fix |
|---------|-----|
| CLAUDE.md asymmetry | Identical treatment: remove for both, or provide a sanitized version to both |
| n=1 | Run n=3-5 per question per group |
| Haiku scorer leniency | Use Sonnet or Opus for scoring |
| No partial credit | Replace binary HIT/MISS with 0/0.5/1 scale per claim |
| Scorer not blind | Strip group-identifying content before scoring (remove `git chronicle` invocations from experiment responses, or score all responses through a uniform template) |
| No leniency calibration | Add 1 "trap" claim per question that the response clearly doesn't address |
| No variance metrics | Report mean, stddev, min, max across runs |
| Only accuracy measured | Also capture: tool calls made, turns used, tokens consumed, wall-clock time |

### Infrastructure

- Automate the eval pipeline end-to-end: `questions.json` → agent runs → scoring → report generation
- Add a `--runs N` parameter to run multiple trials per question
- Store raw agent transcripts (not just final answers) for analyzing *how* agents find information
- Version the evaluation (questions.json should have a schema version for backwards compatibility)

## 7. The Core Takeaway

This eval confirmed one hypothesis and revealed one blindspot:

**Confirmed:** Chronicle provides unique value for knowledge that doesn't
survive into the final code artifact — rejected alternatives, design
rationale, and the "why not" behind decisions. Q4's 0-vs-10 result is the
strongest evidence of this.

**Blindspot:** We don't know if Chronicle helps with questions that are
*hard but not impossible* to answer from code alone. All our non-Q4 questions
were too easy, creating a ceiling that hid any potential Chronicle advantage
on contract awareness, hazard identification, or architecture comprehension.
The next eval must include questions at the right difficulty — hard enough
that the control agent doesn't always score perfectly, but not impossible
(like Q4 was for control).
