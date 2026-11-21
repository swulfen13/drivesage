# Contributing

Thanks for helping! A few ground rules:

1. **Safety is non-negotiable.** DriveSage must never delete user files and every change must stay undoable.
   New features that move data must go through `plan::execute` (journal + undo) and `protect::is_protected`.
2. Run `cargo test -p drivesage-core` and `npm --prefix web run build` before opening a PR.
3. Keep the core logic in `crates/core` (tested), HTTP glue in `crates/server`, UI in `web/`.
4. UI text is German for now; i18n is on the roadmap.

Bug reports: please include your OS, drive types and the steps to reproduce. Never attach private file lists.
