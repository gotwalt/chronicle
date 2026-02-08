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
- The **annotation** explains *why this way*, what constraints to respect,
  and what was already tried and failed

**Do not restate the commit message.** If your summary could be written from
the diff alone, it has no value. Write what the diff cannot tell you.

## When to Annotate

After every commit that adds, modifies, or removes code. Skip only for
lockfiles, generated files, vendored deps, or no-conflict merge commits.

## How to Annotate

Every annotation is a **single Bash command** — do NOT write temp files.

### Default: rich annotation

Before annotating, think about what you know now that will be lost:
- Chose between approaches? → `rejected_alternatives` (saves the next
  agent from the same dead ends)
- Made a design choice? → `decisions` (records what's load-bearing vs.
  provisional)
- Non-obvious invariants? → `markers` (protects the next agent from
  invisible breakage)
- Part of a larger effort? → `effort`

```bash
git chronicle annotate --live << 'EOF'
{
  "commit": "HEAD",
  "summary": "Use exponential backoff for MQTT reconnect — the broker rate-limits reconnects, so rapid retries cause longer lockouts than patient ones.",
  "motivation": "Production logs showed 30s lockouts during network blips.",
  "rejected_alternatives": [
    {"approach": "Jittered fixed interval", "reason": "Still triggers rate limiter when multiple clients reconnect after an outage"}
  ],
  "decisions": [
    {"what": "Cap backoff at 60s", "why": "Balances recovery time vs. user-perceived downtime; matches broker's rate-limit window", "stability": "provisional", "revisit_when": "Broker config becomes tunable"}
  ],
  "markers": [
    {
      "file": "src/mqtt/reconnect.rs",
      "anchor": {"unit_type": "function", "name": "next_delay"},
      "kind": {"type": "contract", "description": "Return value must not exceed MAX_BACKOFF_SECS; callers sleep on this without validation"}
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

### JSON field reference (exact structure — do not deviate)

```
{
  "commit": "HEAD",                              // default HEAD
  "summary": "...",                              // REQUIRED — why, not what
  "motivation": "...",                           // what triggered this change
  "rejected_alternatives": [                     // highest-value field
    {"approach": "...", "reason": "..."}         //   or just a string (auto-converted)
  ],
  "follow_up": "...",                            // expected next steps
  "decisions": [
    {
      "what": "...",                             // REQUIRED per decision
      "why": "...",                              // REQUIRED per decision
      "stability": "permanent",                  // permanent | provisional | experimental
      "revisit_when": "...",                     // optional
      "scope": ["src/foo.rs"]                    // optional
    }
  ],
  "markers": [
    {
      "file": "src/foo.rs",                      // REQUIRED per marker
      "anchor": {"unit_type": "function", "name": "bar"},  // optional
      "kind": {"type": "contract", "description": "..."}   // REQUIRED
    }
  ],
  "effort": {
    "id": "ticket-123",                          // REQUIRED in effort
    "description": "...",                        // REQUIRED in effort
    "phase": "in_progress"                       // start | in_progress | complete
  }
}
```

Marker `kind.type` values: `contract`, `hazard`, `dependency`, `unstable`,
`security`, `performance`, `deprecated`, `tech_debt`, `test_coverage`.

Decision stability: `permanent` (load-bearing), `provisional` (check
`revisit_when`), `experimental` (expect replacement).

## Field Guidance

- **summary**: The *why* behind this approach. Must add information beyond
  the diff and commit message.
- **rejected_alternatives**: Most valuable field. Every dead end you
  document is one the next agent doesn't have to rediscover.
- **markers**: Only where non-obvious. Contracts, hazards, cross-module
  dependencies, security boundaries, performance-critical paths.
- **decisions**: Choices a future agent might question. Tag stability so
  they know how carefully to tread.

## Schema Lookup

```bash
git chronicle schema annotate-input   # input format
git chronicle schema annotation       # stored format
```
