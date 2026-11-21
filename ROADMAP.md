# Roadmap

## v0.1 — MVP ✅
- [x] Drive list (type, file system, used/free)
- [x] Drive benchmark (seq read/write, random 4K, cache bypass)
- [x] Parallel folder scan, stats by type, largest folders/files, old files, duplicates
- [x] Explorer view
- [x] Rule-based plans (sort by type, large+old → slower drive, duplicates report)
- [x] AI plans via Ollama / OpenAI-compatible / Anthropic Claude with strict validation
- [x] Review, edit, dry run, execute, journal, undo
- [x] Setup wizard incl. one-click Ollama model download
- [x] Desktop app (Tauri) + browser mode
- [x] CI + release workflow (Windows/macOS/Linux installers)

## v0.2
- [ ] AI-assisted drive tiering ("which projects/games belong on the fast SSD?")
- [ ] Store API keys in the OS keychain instead of the config file
- [ ] Progress + cancel for running plans, rescan only affected folders after a run
- [ ] English UI / i18n
- [ ] Treemap / sunburst view in the explorer
- [ ] Rename suggestions for meaningless file names (`IMG_2041.jpg` → date-based)

## Later
- [ ] PDF / Office text extraction and image captions (local vision model) for smarter sorting
- [ ] Scheduled "keep Downloads tidy" runs with notifications
- [ ] Windows NTFS MFT fast scan (WizTree-style), SMART health data
- [ ] Duplicate resolver (keep newest / keep in preferred folder → move rest to recycle bin, with undo)
- [ ] Auto-update via Tauri updater
