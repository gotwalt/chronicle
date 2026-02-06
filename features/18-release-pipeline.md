# Feature 18: Automated Release Pipeline + crates.io Publishing

## Overview

Chronicle has no CI, no release automation, and the Cargo.toml package name (`chronicle`) is taken on crates.io. This feature provides:

1. **Package rename** — `chronicle` → `git-chronicle` (available on crates.io, matches binary name)
2. **CI workflow** — fmt, clippy, test on every PR
3. **Automated release** — version bump, changelog, GitHub release, crates.io publish on merge to main
4. **Self-annotation** — Chronicle annotates merge commits in its own release pipeline

**Goal:** Every merge to main automatically produces a versioned release with changelog, crates.io package, and Chronicle annotation.

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 01 CLI & Config | Binary must build and pass tests |
| All prior features | CI validates the full test suite |

---

## Components

### 1. Package Rename (`chronicle` → `git-chronicle`)

The `chronicle` name is taken on crates.io. Renaming to `git-chronicle`:
- Matches the binary name (cargo infers `[[bin]]` when package name = binary name)
- Available on crates.io
- Follows git extension convention (`git-<name>`)

**Cargo.toml changes:**
```toml
[package]
name = "git-chronicle"          # renamed from "chronicle"
version = "0.1.0"
edition = "2021"
description = "AI-powered commit annotation tool that captures reasoning and intent behind code changes"
license = "MIT"
repository = "https://github.com/gotwalt/git-chronicle"
homepage = "https://github.com/gotwalt/git-chronicle"
readme = "README.md"
keywords = ["git", "annotation", "ai", "commit", "developer-tools"]
categories = ["command-line-utilities", "development-tools"]
rust-version = "1.70"
exclude = [".github/", ".claude/", "features/", "HISTORY.md", "CLAUDE.md"]
```

The `[[bin]]` section is removed — when package name matches binary name, cargo infers it automatically.

**LICENSE:** MIT license file added at repo root.

### 2. CI Workflow (`.github/workflows/ci.yml`)

Triggers on pull requests to `main`. Two parallel jobs:

| Job | Steps |
|-----|-------|
| **check** | `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings` |
| **test** | `cargo test --all-features` (with `fetch-depth: 0` for git operations, git user config for test commits) |

Both jobs use:
- `dtolnay/rust-toolchain@stable` with `rustfmt` and `clippy` components
- `Swatinem/rust-cache@v2` for dependency caching

### 3. Release Workflow (`.github/workflows/release.yml`)

Triggers on push to `main`. Skips if commit message starts with `chore(release):` (prevents infinite loop).

**Steps:**

| # | Step | Detail |
|---|------|--------|
| 1 | Checkout | `fetch-depth: 0`, uses `RELEASE_TOKEN` PAT |
| 2 | Install tools | `cargo install cargo-edit git-cliff` |
| 3 | Bump version | `cargo set-version --bump patch` |
| 4 | Update Cargo.lock | `cargo update --workspace` |
| 5 | Generate changelog | `git cliff --output CHANGELOG.md --tag vX.Y.Z` |
| 6 | Commit | `chore(release): vX.Y.Z` (Cargo.toml + Cargo.lock + CHANGELOG.md) |
| 7 | Tag | `vX.Y.Z` |
| 8 | Build | `cargo build --release` |
| 9 | Annotate HEAD~1 | `./target/release/git-chronicle annotate --commit <merge-sha>` with `ANTHROPIC_API_KEY`; `continue-on-error: true` |
| 10 | Push | Commit + tag + chronicle notes |
| 11 | GitHub Release | `gh release create` with `git cliff --latest` as body |
| 12 | Publish | `cargo publish` with `CARGO_REGISTRY_TOKEN` |

**Key design decisions:**

- **Annotates HEAD~1:** The merge commit contains the actual code changes; the release commit is just `chore(release)` metadata.
- **`RELEASE_TOKEN` PAT:** GitHub Actions' `GITHUB_TOKEN` cannot push to protected branches or trigger other workflows. A fine-grained PAT with `contents: write` is required.
- **`continue-on-error: true`:** Annotation failure must not block releases. The LLM API may be unavailable, rate-limited, or the annotation may fail for novel code patterns.
- **`chore(release):` prefix:** The release workflow checks for this prefix and exits early, preventing the release commit from triggering another release.

### 4. git-cliff Configuration (`cliff.toml`)

Groups commits by type for structured changelogs:

| Prefix Pattern | Changelog Section |
|----------------|-------------------|
| `feat:`, `Add ` | Features |
| `fix:`, `Fix ` | Bug Fixes |
| `refactor:`, `Replace `, `Simplify ` | Refactoring |
| `docs:`, `Update ` documentation | Documentation |
| `perf:` | Performance |
| `test:` | Testing |
| `ci:`, `build:` | Build & CI |
| Other | Other Changes |

Skips `chore(release)` commits. Links commits to GitHub.

---

## GitHub Secrets (Manual Setup)

| Secret | Purpose | How to Create |
|--------|---------|---------------|
| `ANTHROPIC_API_KEY` | LLM calls for self-annotation | Anthropic Console → API Keys |
| `CARGO_REGISTRY_TOKEN` | crates.io publishing | `cargo login` → copy token |
| `RELEASE_TOKEN` | Fine-grained PAT with `contents: write` | GitHub → Settings → Developer Settings → Fine-grained PATs |

---

## Files Created/Modified

| File | Action |
|------|--------|
| `features/18-release-pipeline.md` | New — this spec |
| `features/00-overview.md` | Add Feature 18 row |
| `Cargo.toml` | Rename package, add metadata, remove `[[bin]]` |
| `LICENSE` | New — MIT license |
| `.github/workflows/ci.yml` | New — PR checks |
| `.github/workflows/release.yml` | New — automated release |
| `cliff.toml` | New — changelog config |
| `CLAUDE.md` | Update package name reference |

---

## Verification

1. `cargo check` — package rename compiles
2. `cargo test` — all 142+ tests pass
3. `cargo package --list` — verify excluded files aren't in the package
4. `cargo publish --dry-run` — validate crates.io readiness
5. Push a test PR → CI workflow runs fmt, clippy, test
6. Merge to main → release workflow triggers version bump, changelog, GitHub release, crates.io publish

---

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Package rename breaks `use chronicle::` imports | Internal crate name still works; no external consumers yet |
| Release workflow infinite loop | `chore(release):` prefix check exits early |
| Annotation fails in CI | `continue-on-error: true` makes it non-blocking |
| `RELEASE_TOKEN` expires | PAT has configurable expiry; set a calendar reminder |
| crates.io publish fails | Release workflow continues; can retry manually with `cargo publish` |
