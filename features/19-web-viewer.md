# Feature 19: Chronicle Web Viewer

## Status: In Progress

## Summary

A lightweight web viewer that presents Chronicle annotation data alongside source code. Browse the repo file tree, see annotation coverage, and explore annotated regions with their intent, reasoning, constraints, and dependencies.

## Architecture

Express + React/Vite app in `web/`. Single process serves both the API and the frontend. The server shells out to `git` and `git-chronicle` CLI for data — no Rust changes needed.

## API Endpoints

- `GET /api/tree` — File tree at HEAD with per-file annotation counts
- `GET /api/file/:path` — File contents + annotations for a specific file

## Views

### File Browser (v1)
- File tree sidebar with annotation coverage indicators
- Source pane with syntax highlighting (Shiki) and gutter markers for annotated regions
- Annotation pane with region cards showing intent, reasoning, constraints, dependencies, tags, risk notes

### Future Views
- Commit timeline with annotation coverage
- Dependency graph visualization
- Search across annotations

## Stack

| Concern | Choice |
|---------|--------|
| Server | Express 5 |
| Frontend | React 19 + TypeScript |
| Build | Vite 6 |
| Routing | React Router 7 (hash mode) |
| Syntax highlighting | Shiki |
| Styling | Tailwind CSS 4 |
