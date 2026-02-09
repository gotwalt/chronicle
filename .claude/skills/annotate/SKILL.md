# Chronicle Annotate Skill

## Why This Matters

Your context window is temporary. When this session ends, everything you
learned — why you chose this approach, what you tried that failed, what's
fragile — vanishes. Chronicle is your **persistent memory layer**: a place
to store reasoning that survives past your session. The next agent who
touches this code gets your thinking instantly, instead of reverse-engineering
it from the diff.

- The **commit message** says what changed
- The **diff** shows how
- The **annotation** explains *why this way*, what was already tried and
  failed, and what the next agent should watch out for

**Do not restate the commit message.** If your summary could be written from
the diff alone, it has no value. Write what the diff cannot tell you.

## When to Annotate

After every commit that adds, modifies, or removes code. Skip only for
lockfiles, generated files, vendored deps, or no-conflict merge commits.

## How to Annotate

Every annotation is a **single Bash command** — do NOT write temp files.

### Default: rich annotation with wisdom

Before annotating, think about what you know now that will be lost:
- Tried something that didn't work? → `dead_end` (saves the next agent
  from the same mistakes)
- Non-obvious trap or constraint? → `gotcha` (protects the next agent from
  invisible breakage)
- Key insight or mental model? → `insight` (transfers your understanding)
- Work left unfinished? → `unfinished_thread` (tells the next agent what
  to pick up)

```bash
git chronicle annotate --live << 'EOF'
{
  "commit": "HEAD",
  "summary": "Use exponential backoff for MQTT reconnect — the broker rate-limits reconnects, so rapid retries cause longer lockouts than patient ones.",
  "wisdom": [
    {
      "category": "dead_end",
      "content": "Tried jittered fixed interval but it still triggers the broker rate limiter when multiple clients reconnect after an outage.",
      "file": "src/mqtt/reconnect.rs"
    },
    {
      "category": "gotcha",
      "content": "Return value of next_delay() must not exceed MAX_BACKOFF_SECS; callers sleep on this without validation.",
      "file": "src/mqtt/reconnect.rs"
    },
    {
      "category": "unfinished_thread",
      "content": "The backoff cap interacts with the broker's rate-limit window in ways that aren't fully tested under multi-client reconnect storms."
    }
  ]
}
EOF
```

### Minimal: summary-only (typos, renames, dep bumps only)

```bash
git chronicle annotate --summary "Pin serde to 1.0.193 — 1.0.194 has a regression serializing flattened enums (serde-rs/serde#2770)."
```

**Bad** (restates the diff): "Add exponential backoff to reconnect logic"
**Good** (explains why): "Use exponential backoff because the broker rate-limits fixed-interval reconnects, causing longer outages than patient retries"

### JSON field reference (v3 LiveInput — exact structure)

```
{
  "commit": "HEAD",                              // default HEAD
  "summary": "...",                              // REQUIRED — why, not what
  "wisdom": [                                    // optional, default []
    {
      "category": "dead_end",                    // REQUIRED per entry
      "content": "...",                          // REQUIRED per entry
      "file": "src/foo.rs",                      // optional — grounds to file
      "lines": {"start": 10, "end": 25}          // optional — line range
    }
  ]
}
```

Wisdom categories:

| Category | What it captures |
|----------|-----------------|
| `dead_end` | Approaches tried and abandoned — saves future agents from rediscovering failures |
| `gotcha` | Non-obvious traps invisible in the code — constraints, hazards, security boundaries |
| `insight` | Mental models, key relationships, architecture decisions |
| `unfinished_thread` | Incomplete work, suspected better approaches, tech debt |

## Field Guidance

- **summary**: The *why* behind this approach. Must add information beyond
  the diff and commit message.
- **wisdom**: Each entry captures one piece of knowledge that would be lost
  when your session ends. `dead_end` is the highest-value category — every
  dead end you document is one the next agent doesn't have to rediscover.

## Schema Lookup

```bash
git chronicle schema annotate-input   # input format
git chronicle schema annotation       # stored format
```

## Knowledge Accumulation

After annotating, ask: did this commit reveal something that applies beyond
this one commit? If yes, record it in the knowledge store.

### When to add knowledge

| Signal in your annotation | Knowledge type | Command |
|---------------------------|---------------|---------|
| A pattern that applies repo-wide (e.g., all errors use snafu) | Convention | `git chronicle knowledge add --type convention --scope "src/" --rule "..." --stability permanent --decided-in "$(git rev-parse HEAD)"` |
| An approach that should never be used | Anti-pattern | `git chronicle knowledge add --type anti-pattern --pattern "..." --instead "..." --learned-from "$(git rev-parse HEAD)"` |
| A module ownership boundary you enforced or discovered | Boundary | `git chronicle knowledge add --type boundary --module "src/git/" --owns "..." --boundary "..."` |

### When NOT to add knowledge

- One-off design choices → keep as wisdom entries
- Case-specific tradeoffs → keep as dead_end wisdom
- Anything experimental → wait until it proves stable

### Managing knowledge

```bash
git chronicle knowledge list              # see all entries
git chronicle knowledge remove conv-3     # remove by ID
```
