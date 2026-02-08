# Chronicle

## Semantic Memory for Codebases

---

## The Problem

Software has a memory problem. Not in the hardware sense — in the human sense.

Every line of code exists because someone made a decision. They chose this data structure over that one. They added a retry loop because a downstream service fails silently under load. They bounded a cache at four entries because a TLS stack on an embedded device leaks file descriptors beyond that. They wrote the function this way instead of the obvious way because the obvious way triggers a compiler bug on ARM64.

None of this is in the code. The code records *what* was decided. The reasoning — the *why* — lives in the head of whoever wrote it, for as long as they remember, which isn't long. Commit messages capture a fraction. Pull request descriptions capture a bit more. Design documents, if they exist, capture the big decisions but miss the hundreds of small ones. Within weeks, the context that produced a piece of code is gone. What remains is an artifact that works (or appears to) but can't explain itself.

This has always been a problem. It is about to become a much bigger one.

---

## The Shift

AI agents are writing an increasing share of production code. An agent receives a task, reads the relevant files, reasons about the change, writes the code, and commits. It may do this dozens of times a day across a codebase. At the moment of commit, the agent holds everything: the task description, the alternatives it explored and rejected, the constraints it discovered in the code, the implicit contracts it chose to honor, the assumptions it made about other modules.

Then the conversation ends. The agent's context window is cleared. The reasoning is gone — more thoroughly than it ever was with human developers, because a human at least retains some residual memory. An AI agent retains nothing. The next agent to touch that code starts from zero.

This creates a specific, observable failure mode. Agent B is asked to refactor a module. It reads the code, sees a pattern that looks suboptimal, and changes it. The pattern was there because Agent A discovered (and worked around) a subtle interaction with an external system. Agent B's refactor breaks the workaround. The tests pass because the subtle interaction only manifests under production load. The regression ships.

This is not a hypothetical. It is the inevitable result of stateless agents modifying stateful systems. The code is the state, but the code alone doesn't encode enough information to safely modify itself.

---

## What Chronicle Does

Chronicle captures the reasoning behind code changes at the moment they are made and stores it as structured metadata retrievable by future agents working on the same code.

It is a post-commit hook. When a commit is made — by an agent or a human — Chronicle analyzes the change, identifies the semantic units affected (functions, types, modules), and produces a structured annotation for each one. The annotation records: what the change intends to accomplish, what decisions were made and why, what invariants the code protects, what other code depends on assumptions embedded here, and what a future modifier should be cautious about.

These annotations are stored as git notes, keyed by commit SHA. Retrieval uses git blame — the same mechanism git uses to track which commit last touched each line. An agent working on a function runs `git chronicle read`, which blames the lines to find the commits that produced them, fetches the annotations from those commits, and returns structured context: here's why this code exists, here's what it assumes, here's what will break if you change it carelessly.

No external database. No service to run. No index to build. The annotation lives in git, travels with the repository (when configured to sync), and is retrievable with the same tools developers already use.

---

## What an Annotation Contains

An annotation is not a comment or a commit message. It is a structured knowledge artifact designed for machine consumption. For each semantic unit (function, type, configuration block) affected by a commit, the annotation captures:

**Intent.** What the change accomplishes in the context of the broader task. Not "added retry logic" but "the cloud MQTT broker silently drops connections after 30 minutes idle; this implements application-level heartbeats to detect and recover from drops without losing queued messages."

**Reasoning.** What alternatives existed and why this path was chosen. "We considered using MQTT keep-alive but the vendor BSP's TCP stack has a bug where keep-alive timers don't fire reliably on the target SoC. Application-level heartbeats are redundant but reliable."

**Constraints.** Invariants the code protects, stated explicitly. "The message queue must be drained before reconnecting. The broker treats duplicate message IDs from a new session as a protocol violation." An agent that doesn't know this constraint will eventually violate it.

**Semantic dependencies.** Non-obvious couplings to other code. "This function assumes the TLS session cache holds at most 4 sessions. If that bound changes, the reconnection logic here will leak file descriptors." Static analysis can find import-level dependencies. Chronicle captures *behavioral* dependencies — the ones that cause regressions.

**Risk notes.** Anything a future modifier should know. Fragile code paths, performance-sensitive sections, workarounds for external bugs, known technical debt.

**Cross-cutting concerns.** Groups of code that must be updated together, invisible at the single-function level. "If you change the serialization format in `encode()`, you must also update `decode()` and the migration in `v2_compat.rs`."

When the authoring agent provides explicit context (task description, reasoning chain, known dependencies), the annotation is marked `enhanced` and carries the full depth of that reasoning. When the commit comes from a human without explicit context, Chronicle still annotates from diff analysis and code structure — these `inferred` annotations are less rich but still capture more than a commit message ever does.

---

## Use Cases

### Preventing regressions in agentic workflows

The primary use case. Agent A implements a workaround for an external system's quirk and annotates it with the reasoning and constraints. Days later, Agent B is tasked with refactoring the same module. Before making changes, it runs `git chronicle deps` and discovers that three other modules declare behavioral dependencies on the code it's about to modify. It runs `git chronicle read` on the function itself and finds the constraint: "the message queue must be drained before reconnecting." Agent B now knows what Agent A knew. It can refactor safely or choose not to.

Without Chronicle, Agent B has no way to discover this. The code doesn't say "don't change the order of these operations." The tests don't cover the edge case. The commit message says "fix reconnection." The regression happens.

### Onboarding agents to unfamiliar codebases

When an agent (or a new human developer) encounters an unfamiliar module, `git chronicle summary` provides a structural overview: what each function is for, what constraints it operates under, what's risky. This is qualitatively different from reading the code, which tells you what the code *does* but not what it *means*.

A function named `sanitize_input` does something obvious. A function named `normalize_session_id` does something less obvious — and the Chronicle annotation explains that session IDs from the legacy OAuth provider sometimes contain colons, which the message broker interprets as topic separators, so they must be escaped. This context is invisible in the code and critical for anyone modifying the function.

### Preserving reasoning through squash merges

Most teams develop on feature branches with many small commits, then squash-merge to main. This is good for main branch hygiene and terrible for knowledge preservation. Ten commits of incremental reasoning — each explaining a step in the development of a feature — collapse into one commit with a combined diff and a concatenated commit message.

Chronicle detects squash merges and synthesizes the annotations from the source commits into a single consolidated annotation on the squash commit. The reasoning chain is preserved even though the commits that carried it are gone. The annotation records its provenance — "synthesized from 10 commits on feature/mqtt-pooling" — so the Reading Agent knows this is consolidated reasoning rather than a single-commit capture.

### Debugging with a reasoning timeline

When current behavior is surprising, `git chronicle history` shows the reasoning timeline for a code region: not just what changed at each commit, but *why*. This is `git log` for intent. "This timeout was originally 5 seconds. It was raised to 30 because of high-latency satellite links in the field deployment. It was then made configurable because different hardware variants have different network characteristics." An agent debugging a timeout issue now understands the design space rather than guessing.

### Cross-cutting change safety

An agent tasked with "update the serialization format" runs `git chronicle read-multi` across the relevant files and discovers cross-cutting concerns: encode and decode must be updated together, the migration in the compatibility module must be updated, and three other modules declare dependencies on the serialization output format. Without Chronicle, the agent updates `encode()`, the tests for `encode()` pass, and `decode()` silently breaks at runtime.

### Knowledge continuity across team changes

People leave teams. Agents don't have long-term memory. The person who wrote a critical subsystem two years ago is gone. Their design decisions live on in the code but the reasoning is lost. Chronicle annotations are durable knowledge artifacts that outlast any individual contributor. The annotation explaining why the retry policy uses jittered exponential backoff with a specific ceiling is retrievable years later, from the same git history that preserves the code itself.

### A Concrete Example

Agent A implements an MQTT reconnection workaround. The enhanced annotation captures: "The cloud MQTT broker silently drops idle connections after 30 minutes. Application-level heartbeats detect and recover. The message queue MUST be drained before reconnecting — the broker treats duplicate message IDs from a new session as a protocol violation."

Without Chronicle: Agent B is tasked with refactoring the MQTT module a week later. It reads the code, sees a drain-then-reconnect pattern that looks suboptimal — draining the queue before reconnecting adds latency to recovery. Agent B reorders the operations to reconnect-then-drain for "efficiency." Tests pass — the test broker doesn't enforce session-level message ID uniqueness. The regression ships to production and manifests as intermittent protocol violations under load, triggering cascade disconnects across the fleet.

With Chronicle: Agent B runs `git chronicle read` on the reconnection function before modifying it. The annotation surfaces the constraint — drain before reconnect, because of broker protocol semantics. Agent B preserves the operation order, finds a different optimization that doesn't violate the constraint, and the refactor ships safely.

---

## Limitations & When Chronicle Is Not Worth It

**Chronicle is not documentation.** Documentation describes how to use a system from the outside. Chronicle captures the reasoning behind implementation decisions from the inside. They serve different audiences and different needs. Chronicle doesn't replace doc comments, READMEs, or architecture docs.

**Chronicle is not a linter or static analysis tool.** It doesn't enforce rules or catch errors. It provides *context* that helps agents (and humans) make better decisions. The constraint "message queue must be drained before reconnecting" is not a checkable rule — it's knowledge that informs how you approach a modification.

**Chronicle is not a code review tool.** It doesn't approve or reject changes. It annotates them after the fact. A future version might inform code review (surfacing broken constraints or violated dependencies), but that's not the core function.

**Chronicle is not a testing framework.** It complements tests. Tests verify behavior; Chronicle explains intent. A test can tell you that a function returns the right value. Chronicle tells you *why* that value is right and what would make it wrong.

### When Chronicle Is Overhead

Chronicle adds cost (LLM API calls at commit time, storage in git notes, retrieval latency). There are scenarios where the existing context-building behaviors are sufficient:

- **Small, self-evident changes.** Typo fixes, version bumps, import reorders. The annotation agent will skip trivial changes, but the hook still fires.
- **Greenfield code with a single author.** If one person or one agent session is building a new module and will maintain it in the near term, the reasoning is still in their head. Chronicle's value compounds over time and across contributors. Day one, it's overhead.
- **Genuinely self-documenting code.** Pure functions with descriptive names, standard algorithms, simple CRUD operations. Annotations on these would be noise.
- **Rapid prototyping and throwaway code.** If the code won't survive past next week, annotating it wastes API spend. Chronicle should be easy to disable and re-enable when the prototype graduates to production.
- **Very high-frequency commit workflows.** Agents that commit every few seconds generate continuous API calls. Rate limiting and batching mitigate this, but the current architecture isn't optimized for it.

More broadly, Chronicle's marginal value depends on the codebase. It is most valuable for constraint-heavy systems — hardware interactions, protocol edge cases, external system integrations, complex domain logic — where the reasoning behind code is non-obvious and the consequences of ignoring it are severe. It is least valuable for simple CRUD applications with good test coverage, where the code largely explains itself. The honest segmentation is that Chronicle sits on a spectrum of usefulness, and teams should evaluate where their codebase falls on that spectrum rather than assuming universal value.

---

## Architecture in Brief

Chronicle is a single Rust binary. It installs as a set of git hooks (`post-commit`, `prepare-commit-msg`, `post-rewrite`) and provides a CLI with two primary surfaces:

**Writing** (`git chronicle annotate`) — fires at commit time. Gathers the diff, parses affected files with tree-sitter for structural understanding, collects any explicit context from the committing agent via environment variables, and sends the package to an LLM (Anthropic preferred, with fallback to OpenAI, Gemini, or OpenRouter) which produces structured annotations. The annotations are stored as git notes under `refs/notes/chronicle`.

**Reading** (`git chronicle read`, `git chronicle deps`, `git chronicle history`, `git chronicle summary`) — invoked by agents before modifying code. Uses `git blame` to map current code to the commits that produced it, fetches annotations from those commits, scores them by confidence, and returns structured JSON. No LLM calls on the read path. Sub-second latency.

Credential discovery is automatic — it checks for API keys in the environment, looks for Claude subscription credentials on disk, and falls down a provider chain. Installation is `cargo install chronicle && git chronicle init`.

---

## Who This Is For

**Teams using AI agents for code modification.** If agents are committing to your repository, Chronicle is the mechanism by which one agent's reasoning becomes available to the next. Without it, each agent starts from zero.

**Solo developers using Claude Code, Copilot, or similar tools.** Even working alone, if you're delegating coding tasks to an AI, you're creating a knowledge gap between what the AI knew when it wrote the code and what it knows when it comes back to modify it later. Chronicle closes that gap.

**Teams with high code churn in complex domains.** If your codebase has non-obvious constraints — hardware quirks, protocol edge cases, external system behaviors, performance-sensitive paths — and those constraints are currently preserved only in tribal knowledge, Chronicle makes them durable and retrievable.

**Open source projects with rotating contributors.** No single contributor knows the full history of why things are the way they are. Chronicle annotations accumulate a retrievable institutional memory that persists regardless of who's currently active on the project.

---

## The Bet

Chronicle is built on a specific bet about the near future: that AI agents will become the primary authors of code changes, that these agents will operate statelessly across sessions, and that the resulting knowledge loss will become the dominant source of software quality problems.

If that bet is right, the codebase needs a memory system that operates at the same granularity as the code itself — not at the level of documents or tickets, but at the level of functions and types and invariants. And it needs to live where the code lives, in git, retrievable through the same mechanisms that track the code's history.

Chronicle is that memory.

---

## Timing & Ecosystem Risk

The AI coding landscape is moving fast. Persistent agent memory, growing context windows, and platform-level solutions from Anthropic, GitHub, and others could reshape the competitive terrain within a year. Building a standalone tool in this environment carries real risk.

Why building now is the right call despite that uncertainty:

**Git is the most durable substrate in software development.** Twenty years and counting. Build systems change, languages change, editors change, AI providers change. Git persists. Chronicle stores knowledge in git notes — the same transport, the same hosting, the same backup infrastructure. Building on git means building on stability.

**Platform solutions will be walled gardens.** Native memory in Claude Code, Cursor, or Copilot will store context inside the platform, accessible only to that platform's agents. Chronicle stores knowledge in the repository itself, accessible to any agent from any framework. When a team runs Claude Code on Monday and Cursor on Tuesday, Chronicle's annotations are available to both. Platform solutions create lock-in. Chronicle creates a commons.

**Context windows don't solve the problem.** Even if context windows grow to 10 million tokens, the "why" behind code decisions is not *in* the code no matter how much code you read. You can feed an entire repository into a context window and still not know that the reconnection order matters because of a broker protocol constraint. The information was in the author's head at commit time and was never written down. Larger windows help you read more code. They don't help you read reasoning that doesn't exist in text.

**Value compounds over time.** Annotations accumulate. A repository that has been running Chronicle for six months has six months of captured reasoning, ready when most needed — during a refactor, an incident, a new contributor's first week. Starting now means the annotations are there when the codebase reaches the complexity where they become essential.

The risk is real: if platform solutions preempt standalone tools and achieve sufficient adoption, Chronicle's best defense is becoming the standard *format* for stored reasoning, not just a tool. A well-specified annotation schema stored in git — open, portable, framework-agnostic — has value even if the tool that produces the annotations is eventually superseded.