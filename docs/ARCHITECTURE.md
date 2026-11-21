# Architecture

## Components

| Part | Path | Role |
|---|---|---|
| Core | `crates/core` | Pure Rust library: scanning, drives, benchmark, rules, AI, plans, journal/undo |
| Server | `crates/server` | axum HTTP server bound to `127.0.0.1`, JSON API under `/api`, serves the React build embedded with `rust-embed`. Binary: `drivesage-web` |
| Desktop | `src-tauri` | Tauri 2 shell. Starts the server in-process on a random port and opens a native window pointing at it |
| UI | `web` | React 19 + TypeScript + Vite. Talks only to the HTTP API, so the same UI works in the desktop window and in any browser |

## Flow

1. **Scan** (`scan.rs`) walks the selected roots in parallel (`jwalk`) and builds an in-memory index:
   all files (size, mtime, atime, category, protected flag), aggregated folder sizes and parent→children maps.
2. **Drives** (`drives.rs`) come from `sysinfo`; **benchmarks** (`bench.rs`) write/read a temp file with unbuffered,
   page-aligned I/O and are cached in `bench.json`.
3. A **plan** is a list of `Action { from, to, reason, enabled }` plus informational `Finding`s.
   - `rules.rs` builds plans offline.
   - `ai.rs` sends a compact file list to the model and validates the JSON answer (only listed files, only relative
     destinations inside the target, same extension, no duplicates).
4. **Execute** (`plan.rs`) validates every action again right before moving, creates missing folders, moves
   (rename, or copy + verify + remove across drives), and appends each step to `runs/<id>.jsonl` immediately.
   **Undo** replays the journal backwards.

## Data location

`%APPDATA%\DriveSage` (Windows), `~/Library/Application Support/DriveSage` (macOS), `~/.config/DriveSage` (Linux):
`config.json`, `bench.json`, `runs/*.jsonl` + `runs/*.report.json`.

## API (excerpt)

All routes need the header `x-drivesage-token`.

| Method | Route | |
|---|---|---|
| GET/PUT | `/api/config` | settings (API key is masked when read) |
| GET | `/api/drives` | drives incl. benchmark results |
| POST | `/api/bench` | `{drive_id, size_mb}` → job |
| POST | `/api/scan` | `{roots}` → job (result = summary) |
| GET | `/api/scan/tree?path=` | folder children by size |
| POST | `/api/plan/rules` | `{target, options}` → plan |
| POST | `/api/plan/ai` | `{target}` → job (result = plan) |
| PUT | `/api/plans/{id}` | toggle / edit actions |
| POST | `/api/plans/{id}/execute` | `{dry_run}` → report |
| GET | `/api/runs` · POST `/api/runs/{id}/undo` | history & undo |
| GET | `/api/jobs/{id}` | progress of background jobs |
| GET/POST | `/api/ai/ollama/models` · `/api/ai/ollama/pull` · `/api/ai/test` | AI setup helpers |
