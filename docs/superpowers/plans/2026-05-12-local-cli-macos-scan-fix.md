# Local CLI macOS Scan Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix macOS local CLI scanning so versions/descriptions are populated where metadata is available, Homebrew dependencies are excluded, and the Rescan button always performs a visible fresh scan.

**Architecture:** Keep the scanner metadata-first and avoid executing arbitrary discovered CLIs. Add explicit Homebrew metadata helpers around stable `brew` commands and make frontend rescan a first-class action that resets local description fetch state before forcing React Query to refetch.

**Tech Stack:** Rust/Tauri commands, rusqlite cache, React + TanStack Query, Vitest, Cargo tests.

---

## Diagnosis Summary

1. Version/description gaps:
   - `detect_version()` only supports npm, pnpm, and pip. Brew/scoop/choco return `None` in `src-tauri/src/services/local_cli_scanner.rs:233-242`.
   - `resolve_description()` only supports npm, pnpm, and pip. Brew tools return `None` in `src-tauri/src/services/local_cli_scanner.rs:277-283`.
   - npm/pnpm/pip are intentionally metadata-only. That is good for safety, but it means path resolution must be correct and brew needs its own metadata source.

2. Brew dependencies are included:
   - The scanner adds `/opt/homebrew/bin` and `/usr/local/bin` unconditionally in `common_cli_search_dirs()`.
   - Filtering depends on `Command::new("brew")` succeeding in `brew_installed_on_request_formulae()`.
   - If the GUI-launched Tauri process cannot find `brew`, the function returns `None`, and `filter_brew_executables_to_installed_on_request()` returns all brew paths unchanged.
   - If a brew path cannot be mapped to `/Cellar/<formula>/...`, the current filter uses `unwrap_or(true)`, which keeps the path. For brew-managed paths this is too permissive.

3. Rescan looks ineffective:
   - `useLocalCliTools()` sets `staleTime: 60_000` and `refetchOnMount: false` in `src/hooks/useLocalCli.ts:22-30`.
   - The button calls `refetch()` directly in `src/components/LocalCliPage.tsx:233`, but there is no local rescan pending state and it does not clear `descriptionMap` or `attemptedDescriptionIdsRef`.
   - After a failed lazy description attempt, `attemptedDescriptionIdsRef` prevents retrying the same tool until the component remounts.

4. Test baseline issue:
   - `cargo test local_cli --manifest-path src-tauri/Cargo.toml` currently fails 6 tests on macOS because tests assume commands like `npm`/`python3` are not resolved to `/opt/homebrew/bin/...`, and some executable test fixtures have no Unix execute bit.

## Files

- Modify: `src-tauri/src/services/local_cli_scanner.rs`
- Modify: `src-tauri/src/commands/local_cli.rs`
- Modify: `src/hooks/useLocalCli.ts`
- Modify: `src/components/LocalCliPage.tsx`
- Modify: `src/components/LocalCliPage.test.tsx`
- Modify: `src/hooks/useLocalCli.test.tsx`
- Optional: `src-tauri/src/services/local_cli_updater.rs`

## Task 1: Stabilize Local CLI Test Fixtures

- [ ] Update scanner tests that create fake Unix executables to set executable permissions under `#[cfg(unix)]`.
- [ ] Update command-building tests to assert the executable basename when the code intentionally resolves to a real package manager on the host.

Example helper to add inside `src-tauri/src/services/local_cli_scanner.rs` tests:

```rust
fn write_executable(path: &Path, content: &[u8]) {
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
```

Example command test helper in `src-tauri/src/commands/local_cli.rs` tests:

```rust
fn command_name(path_or_name: &str) -> String {
    Path::new(path_or_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path_or_name)
        .to_string()
}
```

- [ ] Run:

```bash
cargo test local_cli --manifest-path src-tauri/Cargo.toml
```

Expected: tests that fail today due to local environment assumptions should pass before behavior changes are added.

## Task 2: Make Brew Discovery Deterministic on macOS

- [ ] Add `find_brew_command()` in `src-tauri/src/services/local_cli_scanner.rs`.

```rust
fn find_brew_command() -> Option<PathBuf> {
    which::which("brew")
        .ok()
        .or_else(|| ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].iter().map(PathBuf::from).find(|p| p.is_file()))
}
```

- [ ] Change `brew_installed_on_request_formulae()` to call the resolved path instead of `Command::new("brew")`.
- [ ] Add a fallback to `brew leaves` only if `brew list --formula --installed-on-request` fails.
- [ ] Change brew filtering so unresolved brew formula names are excluded, not kept.

```rust
brew_formula_name_from_path(path)
    .is_some_and(|formula| installed_on_request.contains(&formula))
```

- [ ] Add tests:
  - `filter_brew_executables_drops_unresolved_brew_paths_when_filter_available`
  - `brew_formula_name_from_path_resolves_opt_homebrew_symlink`

- [ ] Run:

```bash
cargo test local_cli_scanner --manifest-path src-tauri/Cargo.toml
```

Expected: brew dependency executables like `xz`/`lzmainfo` are excluded when they are not top-level installs.

## Task 3: Add Brew Version and Description Metadata

- [ ] Add helpers that resolve the formula name from the path and query Homebrew metadata:

```rust
fn detect_brew_version(path: &Path) -> Option<String> {
    let formula = brew_formula_name_from_path(path)?;
    let output = run_brew(["list", "--versions", &formula])?;
    output.split_whitespace().nth(1).map(ToOwned::to_owned)
}

fn resolve_brew_description(path: &Path) -> Option<String> {
    let formula = brew_formula_name_from_path(path)?;
    let output = run_brew(["desc", "--formula", &formula])?;
    output
        .trim()
        .split_once(": ")
        .map(|(_, desc)| desc.trim().to_string())
        .filter(|desc| !desc.is_empty())
}
```

- [ ] Wire `PackageManager::Brew` into `detect_version()` and `resolve_description()`.
- [ ] Keep scoop/choco unchanged unless there is a confirmed macOS need.
- [ ] Add tests with fixture paths and command parsing helpers. If direct command mocking is not present, split parsing into pure functions:

```rust
fn parse_brew_list_versions_output(output: &str) -> Option<String>
fn parse_brew_desc_output(output: &str) -> Option<String>
```

- [ ] Run:

```bash
cargo test local_cli_scanner --manifest-path src-tauri/Cargo.toml
```

Expected: brew tools can show current version and description without executing the discovered CLI binary.

## Task 4: Make Rescan a Forced Refresh in the UI

- [ ] In `src/hooks/useLocalCli.ts`, either:
  - expose a `useRescanLocalCliTools()` mutation that calls `api.listLocalCliTools()` and writes `LOCAL_CLI_QUERY_KEY`, or
  - keep `refetch()` but return/use `isFetching` so the button has correct pending state.

Recommended mutation:

```ts
export function useRescanLocalCliTools() {
  const qc = useQueryClient();
  return useMutation<LocalCliTool[], Error, void>({
    mutationFn: () => api.listLocalCliTools(),
    onSuccess: (data) => qc.setQueryData(LOCAL_CLI_QUERY_KEY, data),
  });
}
```

- [ ] In `src/components/LocalCliPage.tsx`, replace direct button `refetch()` with `handleRescan()`.
- [ ] `handleRescan()` should:
  - clear `descriptionMap`
  - clear `attemptedDescriptionIdsRef.current`
  - clear `fetchProgress`
  - call the rescan mutation
  - show loading state on the button while rescan is pending

```ts
const handleRescan = () => {
  setDescriptionMap({});
  attemptedDescriptionIdsRef.current.clear();
  setFetchProgress(null);
  rescanTools();
};
```

- [ ] Keep the existing `refetch()` at the end of lazy description fetch, because it refreshes DB-backed descriptions after successful writes.
- [ ] Add a Vitest test that clicks the rescan button and asserts:
  - `api.listLocalCliTools` or the rescan mutation is called
  - a previously attempted missing description can be retried after rescan

- [ ] Run:

```bash
pnpm exec vitest run src/components/LocalCliPage.test.tsx src/hooks/useLocalCli.test.tsx src/lib/local-cli.test.ts
```

Expected: rescan behavior is covered and frontend tests remain green.

## Task 5: Verify End to End on macOS

- [ ] Run:

```bash
cargo test local_cli --manifest-path src-tauri/Cargo.toml
pnpm exec vitest run src/components/LocalCliPage.test.tsx src/hooks/useLocalCli.test.tsx src/lib/local-cli.test.ts
pnpm typecheck
```

- [ ] Start the app:

```bash
pnpm dev
```

- [ ] On macOS, verify manually:
  - Local CLI page initially lists top-level Homebrew formula tools, not dependency formula tools.
  - Brew rows show version/description where Homebrew metadata exists.
  - npm/pnpm/pip rows still show metadata-derived version/description.
  - Clicking Rescan visibly enters pending state and refreshes the list.
  - A tool whose description failed earlier is retried after Rescan.

## Risks and Notes

- Avoid executing discovered arbitrary CLI paths for `--version` or `--help`; package shims can be user-controlled.
- Homebrew may expose several binaries for one top-level formula. The current UI models binaries as tools, so filtering dependencies does not dedupe multiple binaries from the same requested formula. Dedupe-by-formula is a separate product decision.
- `brew desc` and `brew list --versions` are local commands but can still be slow if Homebrew auto-update is triggered. Set `HOMEBREW_NO_AUTO_UPDATE=1` on metadata commands if latency appears in manual testing.
