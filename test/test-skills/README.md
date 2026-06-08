# Security scanner end-to-end fixtures

This directory contains directory-shaped skill fixtures used by `src-tauri/tests/scan_test_skills.rs`.

- `positive-real/` contains text-only snapshots of installed local skills. These are expected to scan without install-blocking hard triggers.
- `negative-generated/` contains generated counterexample skills. Each fixture may include `expected.json` with required rule IDs and blocking expectations.

The positive fixtures intentionally exclude binaries, dependency caches, virtual environments, and files larger than 256 KiB so the repository stays small and deterministic.
