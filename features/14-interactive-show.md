# Feature 14: Interactive Show

## TUI Explorer for Annotated Source Code

---

## 1. Overview

Feature 14 adds `git chronicle show` — an interactive terminal UI for exploring annotated source code. It renders a file with annotation coverage indicators and lets the user drill into intent, reasoning, constraints, dependencies, and risk notes on a region-by-region basis.

Think of it as `git blame` for *intent* — but interactive, navigable, and rich.

**Entry point:**

```
git chronicle show [OPTIONS] <PATH> [<ANCHOR>]
```

**Key behaviors:**

- Renders source code with a gutter indicating annotation coverage per line
- Selecting a region reveals its annotation: intent, reasoning, constraints, dependencies, risk notes, corrections
- Keyboard navigation: scroll, expand/collapse, jump between annotated regions
- Drill-down: press keys to pivot into `deps`, `history`, or related annotations without leaving the TUI
- Falls back to a non-interactive annotated listing when stdout is not a TTY (piping, CI)
- Behind a `tui` Cargo feature flag, enabled by default

**Dependencies:**

| Feature | What it provides |
|---------|-----------------|
| 07 Read Pipeline | Note fetching, region filtering |
| 08 Advanced Queries | deps, history, summary data |
| 03 AST Parsing | Outline extraction for structural navigation |
| 02 Git Operations | Blame, notes, file-at-commit |

---

## 2. Public API

### 2.1 CLI

```
git chronicle show [OPTIONS] <PATH> [<ANCHOR>]
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `PATH` | Yes | File path relative to repo root |
| `ANCHOR` | No | AST anchor to focus on (function, struct, etc.) |

**Options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--commit` | `String` | `HEAD` | Show file at this commit |
| `--no-tui` | `bool` | `false` | Force non-interactive (plain text) output |
| `--theme` | `String` | `auto` | Color theme: `auto`, `dark`, `light`, `none` |
| `--blame` | `bool` | `false` | Show blame SHAs in gutter alongside annotation markers |

When `--no-tui` is passed or stdout is not a TTY, output falls back to a plain-text annotated listing (see Section 5).

### 2.2 Behavior

```
git chronicle show src/provider/anthropic.rs
```

Launches the TUI. The user sees the file with a gutter column showing annotation status per line:

```
 ┃ src/provider/anthropic.rs                              [3 regions annotated]
 ┃
 ▐ 1  use serde::{Deserialize, Serialize};
 ▐ 2  use snafu::ResultExt;
 ▐ 3
 █ 4  const API_URL: &str = "https://api.anthropic.com/v1/messages";
 █ 5  const ANTHROPIC_VERSION: &str = "2023-06-01";
 █ 6  const MAX_RETRIES: u32 = 3;
 ▐ 7
 █ 8  pub struct AnthropicProvider {              ┃ intent: Core Anthropic API client
 █ 9      api_key: String,                        ┃   wrapping ureq for blocking HTTP
 █10      model: String,                           ┃   calls with retry logic.
 █11      agent: ureq::Agent,                      ┃
 █12  }                                            ┃ [Enter] expand  [d]eps  [h]istory
```

The `█` gutter indicates annotated lines. When the cursor moves to an annotated region, the annotation panel appears on the right showing intent. Pressing Enter expands to show full detail.

### 2.3 With `--anchor`

```
git chronicle show src/provider/anthropic.rs AnthropicProvider::complete
```

Opens the TUI scrolled to and focused on the `complete` method, with its annotation expanded.

---

## 3. TUI Layout

### 3.1 Screen Regions

```
┌─────────────────────────────────────────────────────────────────────┐
│  src/provider/anthropic.rs @ HEAD (566a553)       [q]uit [?]help   │  ← header
├──┬──────────────────────────────────┬───────────────────────────────┤
│▒▒│ source code                      │ annotation panel              │  ← main
│▒▒│ with line numbers                │ (intent, reasoning,           │
│▒▒│                                  │  constraints, deps,           │
│▒▒│                                  │  risk notes, corrections)     │
│▒▒│                                  │                               │
├──┴──────────────────────────────────┴───────────────────────────────┤
│  region 2/3 │ AnthropicProvider::complete │ lines 176-261 │ 2 deps  │  ← status
└─────────────────────────────────────────────────────────────────────┘
```

- **Header**: file path, commit, keybinding hints
- **Gutter**: annotation coverage indicator (2 chars wide)
- **Source pane**: syntax-highlighted source code with line numbers
- **Annotation pane**: details for the selected region (collapsible)
- **Status bar**: current region index, anchor name, line range, stats

### 3.2 Gutter Indicators

| Symbol | Meaning |
|--------|---------|
| `█` (solid) | Line covered by an annotated region |
| `▓` (dark) | Line covered by a region with corrections/flags |
| `░` (light) | Line covered by a region with risk notes |
| ` ` (blank) | No annotation coverage |

Colors map to confidence when available: green (high), yellow (medium), red (low/flagged).

### 3.3 Annotation Panel Sections

When a region is selected, the panel shows sections in order:

1. **Intent** (always shown) — the primary explanation
2. **Reasoning** (if present) — why this approach was chosen
3. **Constraints** (if present) — design invariants, with source (Author/Inferred)
4. **Dependencies** (if present) — semantic dependencies with file, anchor, nature
5. **Risk notes** (if present) — known risks, fragility
6. **Corrections** (if present) — flagged issues and applied corrections
7. **Metadata** — commit SHA, timestamp, context level, provenance, tags

Sections are collapsible. The panel scrolls independently from the source pane.

---

## 4. Keyboard Navigation

### 4.1 Movement

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll source down one line |
| `k` / `Up` | Scroll source up one line |
| `Ctrl-d` / `Page Down` | Scroll down half screen |
| `Ctrl-u` / `Page Up` | Scroll up half screen |
| `g` / `Home` | Jump to top of file |
| `G` / `End` | Jump to bottom of file |
| `n` | Jump to next annotated region |
| `N` | Jump to previous annotated region |
| `/` | Search text in source |
| `Tab` | Cycle focus: source pane <-> annotation pane |

### 4.2 Region Interaction

| Key | Action |
|-----|--------|
| `Enter` | Toggle expand/collapse annotation panel |
| `d` | Show dependencies for current region (pivots to deps view) |
| `h` | Show history for current region (pivots to history view) |
| `r` | Show related annotations |
| `1`-`7` | Toggle annotation panel sections (intent, reasoning, etc.) |

### 4.3 Views

| Key | Action |
|-----|--------|
| `s` | Toggle to summary view (all regions, condensed) |
| `b` | Toggle blame SHAs in gutter |
| `Esc` | Return from sub-view (deps/history) to source |
| `q` | Quit |
| `?` | Toggle help overlay |

### 4.4 Drill-Down Views

Pressing `d` on a region opens a deps sub-view within the TUI:

```
┌─────────────────────────────────────────────────────────────────────┐
│  deps: AnthropicProvider::complete                   [Esc] back     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. src/agent/mod.rs :: run_agent_loop                              │
│     "Calls complete() which is now synchronous"                     │
│     commit: 566a553 (2026-02-06)                                    │
│                                                                     │
│  2. src/annotate/mod.rs :: run                                      │
│     "Calls run_agent_loop which calls provider.complete()"          │
│     commit: 566a553 (2026-02-06)                                    │
│                                                                     │
│  [Enter] navigate to dependent  [Esc] back to source               │
└─────────────────────────────────────────────────────────────────────┘
```

Pressing Enter on a dependent opens that file in the show view (pushes onto a navigation stack). Esc pops back.

Similarly, `h` shows the history timeline for the current region, and Enter on a history entry navigates to that commit's annotation.

---

## 5. Non-Interactive Fallback

When stdout is not a TTY or `--no-tui` is passed, output is a plain-text annotated listing:

```
src/provider/anthropic.rs @ HEAD (566a553)

  4-12  AnthropicProvider (struct)
        intent:  Core Anthropic API client wrapping ureq for blocking HTTP
        constraints:
          - ureq v2 required (v3 has different API) [author]
          - tls feature enables native TLS for HTTPS [author]
        deps: (none)
        risk: (none)

 176-261 AnthropicProvider::complete (impl)
        intent:  Replace reqwest::Client with ureq::Agent and rewrite HTTP
                 call pattern from async to blocking
        reasoning: ureq uses match on Ok/Err(Status)/Err(Transport) instead
                   of reqwest status checks after await
        constraints:
          - Retry logic handles 429 and 5xx via Err(ureq::Error::Status) [author]
          - Transport errors boxed and wrapped via snafu .context(HttpSnafu) [author]
        deps:
          -> src/error.rs :: ProviderError::Http
          -> src/provider/mod.rs :: LlmProvider
        risk: (none)
```

This is useful for piping into other tools, CI logs, or MCP tool responses.

---

## 6. Internal Design

### 6.1 Data Pipeline

```
1. Resolve PATH relative to repo root.
2. Read file content at --commit (git show <commit>:<path>).
3. Parse file with tree-sitter → AST outline (units with line ranges).
4. Fetch all annotations for the file:
   a. Walk git log for commits that touched this file.
   b. For each commit, read chronicle note if present.
   c. Filter regions matching this file.
   d. Sort by line range.
5. Build a LineAnnotationMap: for each source line, which region(s) cover it.
6. Render initial view.
```

The `LineAnnotationMap` is the core data structure connecting source lines to annotation regions:

```rust
/// Maps each source line to its annotation coverage.
pub struct LineAnnotationMap {
    /// For each line number (1-indexed), the region(s) covering it.
    line_regions: Vec<Vec<RegionRef>>,
}

pub struct RegionRef {
    pub region: RegionAnnotation,
    pub commit: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub context_level: ContextLevel,
    pub provenance: Provenance,
}
```

### 6.2 Module Structure

```
src/
├── show/
│   ├── mod.rs           # Public API: build_show_data(), run_tui(), run_plain()
│   ├── data.rs          # LineAnnotationMap, data pipeline
│   ├── tui.rs           # Ratatui app loop, layout, rendering (behind feature flag)
│   ├── plain.rs         # Non-interactive plain-text renderer
│   ├── keymap.rs        # Key event → action mapping
│   └── views/
│       ├── source.rs    # Source pane with gutter
│       ├── annotation.rs # Annotation detail panel
│       ├── deps.rs      # Deps drill-down sub-view
│       ├── history.rs   # History drill-down sub-view
│       └── help.rs      # Help overlay
├── cli/
│   └── show.rs          # CLI command definition
```

### 6.3 App State

```rust
pub struct ShowApp {
    /// Source file content (lines).
    source_lines: Vec<String>,
    /// AST outline for structural navigation.
    outline: Vec<OutlineEntry>,
    /// Annotation data mapped to lines.
    annotation_map: LineAnnotationMap,
    /// Sorted list of annotated regions for n/N navigation.
    regions: Vec<RegionRef>,

    /// Current scroll offset in source pane.
    scroll_offset: usize,
    /// Currently selected region index (if any).
    selected_region: Option<usize>,
    /// Whether annotation panel is expanded.
    panel_expanded: bool,
    /// Which panel sections are visible.
    visible_sections: SectionMask,
    /// Active sub-view (None = source, Some = deps/history/related).
    sub_view: Option<SubView>,
    /// Navigation stack for drill-down (file, anchor, scroll).
    nav_stack: Vec<NavEntry>,

    /// Focus: source pane or annotation pane.
    focus: Pane,
    /// Terminal dimensions.
    area: Rect,
}

pub enum SubView {
    Deps(DepsViewState),
    History(HistoryViewState),
    Related(RelatedViewState),
    Help,
}
```

### 6.4 Rendering

The TUI uses ratatui with a crossterm backend. The render loop:

1. Clear screen
2. Render header (file path, commit, key hints)
3. Split main area horizontally: source pane (60-70%) + annotation pane (30-40%)
4. Source pane: gutter (2 chars) + line numbers (width auto) + source text
5. Annotation pane: render sections for the selected region
6. Status bar: region info, navigation state
7. If sub-view is active, render it as an overlay or replacement for the annotation pane

**Syntax highlighting**: Use the tree-sitter parse tree that's already available for outline extraction. Walk the tree to assign token types to byte ranges, map to terminal colors. This avoids adding a syntect/bat dependency.

### 6.5 Conditional Compilation

```toml
# Cargo.toml
[features]
default = ["tui"]
tui = ["ratatui", "crossterm"]

[dependencies]
ratatui = { version = "0.30", optional = true, default-features = false, features = ["crossterm"] }
crossterm = { version = "0.28", optional = true }
```

The `show` command is always available. When the `tui` feature is disabled:
- `git chronicle show` produces the plain-text fallback
- No ratatui/crossterm in the dependency tree
- The `src/show/tui.rs` and `src/show/views/` modules are gated with `#[cfg(feature = "tui")]`

```rust
// src/cli/show.rs
pub fn run(path: String, anchor: Option<String>, opts: ShowOpts) -> Result<()> {
    let data = show::build_show_data(&git_ops, &path, anchor.as_deref(), &opts)?;

    if opts.no_tui || !atty::is(atty::Stream::Stdout) {
        show::run_plain(&data, &mut std::io::stdout())
    } else {
        #[cfg(feature = "tui")]
        {
            show::run_tui(data, &opts)
        }
        #[cfg(not(feature = "tui"))]
        {
            show::run_plain(&data, &mut std::io::stdout())
        }
    }
}
```

Note: Use `std::io::IsTerminal` (stable since Rust 1.70) instead of the `atty` crate to avoid an extra dependency.

---

## 7. Error Handling

| Failure Mode | Behavior |
|--------------|----------|
| File not found | Error: "file not found: {path}" with suggestion if similar path exists |
| File has no annotations | Show source code with empty gutter, status bar: "no annotations" |
| Anchor not found | Show file, scroll to best match or top, warn in status bar |
| Terminal too small | Collapse annotation panel; below 40 cols, fall back to plain output |
| Tree-sitter parse fails | Show source without structural navigation; annotations still work |
| Feature `tui` disabled | Always use plain-text fallback |
| Piped output | Detect non-TTY, use plain-text fallback |

---

## 8. Configuration

```ini
[chronicle]
    # Default theme for TUI
    showTheme = auto

    # Whether to show blame SHAs by default
    showBlame = false

    # Annotation panel width as percentage (30-70)
    showPanelWidth = 40
```

---

## 9. Implementation Steps

### Step 1: Data Pipeline (`src/show/data.rs`)
**Scope:** Implement `build_show_data()` — reads the file, parses AST outline, fetches annotations, builds `LineAnnotationMap`. Reuse existing `read::retrieve` infrastructure. No TUI code yet. Tests: map construction, multi-region coverage, empty file, file with no annotations.

### Step 2: Plain-Text Renderer (`src/show/plain.rs`)
**Scope:** Implement `run_plain()` — formats the annotated listing for non-interactive output. Tests: output format matches spec, handles all annotation fields, empty regions.

### Step 3: CLI Command (`src/cli/show.rs`)
**Scope:** Add `Show` variant to `Commands` enum, wire up argument parsing. Route to `run_plain()` initially. Tests: CLI argument parsing, `--no-tui` flag.

### Step 4: TUI Shell (`src/show/tui.rs`)
**Scope:** Set up ratatui app loop with crossterm backend. Render header, source pane with gutter, status bar. Scrolling and basic navigation (j/k/g/G). No annotation panel yet. Tests: manual (TUI is hard to unit test); verify app launches and exits cleanly.

### Step 5: Annotation Panel (`src/show/views/annotation.rs`)
**Scope:** Render the annotation detail panel for the selected region. All 7 sections with expand/collapse. Tab to switch focus. Tests: section rendering, collapsible sections.

### Step 6: Region Navigation
**Scope:** Implement `n`/`N` to jump between annotated regions. Auto-select region when cursor enters an annotated line range. Update status bar with region info.

### Step 7: Deps Drill-Down (`src/show/views/deps.rs`)
**Scope:** Press `d` to show dependencies sub-view. Reuse `read::deps` pipeline. Enter to navigate to a dependent file (push onto nav stack). Esc to pop back.

### Step 8: History Drill-Down (`src/show/views/history.rs`)
**Scope:** Press `h` to show history sub-view. Reuse `read::history` pipeline. Enter to navigate to a historical commit. Esc to pop back.

### Step 9: Syntax Highlighting
**Scope:** Use tree-sitter parse tree to assign token colors to source lines. Map tree-sitter node types to a small palette (keyword, string, comment, type, function, etc.). Respect `--theme` flag.

### Step 10: Summary View
**Scope:** Press `s` to toggle summary view — shows all regions condensed (one line per region: anchor, intent snippet, coverage %). Useful for orientation before drilling into specific regions.

---

## 10. Test Plan

### Unit Tests

**Data pipeline:**
- File with 3 annotated regions across 2 commits: `LineAnnotationMap` has correct coverage.
- Line covered by two overlapping regions: both appear in map.
- File with zero annotations: empty map, all gutter indicators blank.
- Anchor filter narrows to single region.
- Commit filter shows annotation state at a historical commit.

**Plain-text renderer:**
- Output matches expected format for a file with mixed annotated/unannotated regions.
- All annotation fields rendered when present.
- Optional fields (reasoning, risk_notes) omitted cleanly when absent.
- Corrections section appears when corrections exist.

**LineAnnotationMap:**
- `region_at_line(n)` returns correct region(s).
- `next_region_from(n)` returns the next annotated line after n.
- `prev_region_from(n)` returns the previous annotated region before n.

### Integration Tests

**End-to-end plain output:**
1. Create a repo with annotated commits.
2. Run `git chronicle show --no-tui src/file.rs`.
3. Verify output contains source regions with intent/constraints.

**Feature flag:**
1. Build with `--no-default-features` (disables `tui`).
2. Run `git chronicle show src/file.rs`.
3. Verify plain-text output (no panic, no ratatui dependency).

### Manual Tests

TUI rendering is validated manually:
- Launch on files of various sizes (10 lines, 500 lines, 5000 lines).
- Verify scrolling performance stays smooth.
- Test terminal resize handling.
- Test on 80x24 minimum terminal size.
- Verify color rendering on dark and light terminal backgrounds.

---

## 11. Acceptance Criteria

1. `git chronicle show src/file.rs` launches an interactive TUI showing annotated source code with a gutter, annotation panel, and keyboard navigation.

2. `n`/`N` keys jump between annotated regions. The annotation panel updates to show the selected region's details.

3. `d` and `h` keys open deps and history sub-views for the selected region, with Enter to navigate and Esc to return.

4. `git chronicle show --no-tui src/file.rs` (and piped output) produces a readable plain-text annotated listing.

5. Building with `cargo build --no-default-features` excludes ratatui/crossterm from the dependency tree. The `show` command still works with plain-text output.

6. The TUI handles files with zero annotations gracefully (shows source, empty gutter, informative status bar).

7. Terminal resize is handled without crash or corruption.

8. The data pipeline reuses existing read, deps, history, and AST infrastructure — no duplicated logic.

9. Startup time for a 500-line file with 10 annotated regions is <500ms.

10. The plain-text fallback output is useful for piping into other tools (grep, less, MCP responses).
