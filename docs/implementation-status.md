# Code Base Implementation Status

Baseline spec: [code-base-v1-spec.md](code-base-v1-spec.md)

Last reviewed commit: working tree after local install scripts and release hardening docs

## Summary

Code Base currently has the v1 CLI and TUI workflows implemented. The project can be built as Rust binaries named `cb` and `cb-tui`, store project metadata in SQLite, manage TOML config, register/search/edit projects, open projects in an editor, view/edit/create project docs, delete projects with guardrails, browse projects in a two-pane TUI, and install/uninstall locally through distro-neutral shell scripts.

The remaining gaps are release polish-level: decide whether to add signed/tagged binary release artifacts, richer terminal Markdown styling, and future TUI refinements as patches.

The implementation is now organized as a library crate with small binary entrypoints and internal modules for storage/config, TUI/tree/editor/doc helpers, and tests.

## Status By Area

| Area | Status | Notes |
| --- | --- | --- |
| V1 spec | Done | Product decisions are captured in `docs/code-base-v1-spec.md`. |
| Rust crate | Done | Cargo binary crate exists with package `codebase` and binary targets `cb` and `cb-tui`. |
| Code structure | Done | `src/main.rs` and `src/bin/cb-tui.rs` are thin entrypoints; shared logic lives in `src/lib.rs`, `src/commands.rs`, `src/storage.rs`, `src/tui.rs`, and `src/tests.rs`. |
| Dependency lockfile | Done | `Cargo.lock` is committed for reproducible app builds. |
| Release docs | Done | README, install docs, and a release checklist are present. |
| CI | Done | GitHub Actions runs script syntax checks, Rust checks, and an install smoke. |
| Config model | Done | TOML config defaults and `config get/set` are implemented. |
| SQLite registry | Done | Embedded schema creates `projects`, `tags`, and `project_tags`. |
| Project IDs | Done | Stable random `cb_...` IDs are generated with collision checks. |
| Path handling | Done | Registration canonicalizes existing paths and enforces unique stored paths. |
| Git detection | Done | Init-time containing Git root detection is stored and shown as an indicator. |
| Tags | Done | Tags are lowercase ASCII slugs with replacement/add/remove edit paths. |
| Search and selector resolution | Done | Metadata search and interactive ambiguity picker are reused by CLI workflows. |
| Table and JSON output | Done | `list`, `search`, and `show` support human/JSON output where planned. |
| Interactive prompts | Done | CLI prompts and TUI interactions exist for current v1 workflows. |
| Editor resolution | Done | Project/global editor config, one-time overrides, direct execution, shell templates, and `{path}` substitution exist. |
| Documentation path metadata | Done | `doc_path` is stored, validated, resolved, viewed, edited, and created through `cb doc`. |
| Missing project handling | Done | CLI and TUI show recoverable missing-path/missing-doc states. |
| `open` command | Done | Launches configured editor, refuses missing paths, and updates `last_opened_at` after spawn. |
| `doc` command | Done | Supports path/raw/rendered view, edit, create, and parent directory creation. |
| `delete` command | Done | Supports trash-first deletion, permanent deletion guardrails, and missing-path cleanup. |
| TUI | Done | Two-pane Ratatui launcher with search, tabs, sort controls, status bar, keyboard, and mouse support exists. |
| Docs rendering | Partial | CLI and TUI use basic terminal-native Markdown rendering; richer styling remains future polish. |
| File tree preview | Done | Live tree generation respects config limits, ignore rules, hidden filtering, and truncation markers. |
| Install scripts | Done | `scripts/install.sh` and `scripts/uninstall.sh` install/remove `cb` and `cb-tui` through Cargo. |

## Implemented Commands

These commands are currently exposed by `cb --help`:

- `cb init`
- `cb list`
- `cb search`
- `cb open`
- `cb show`
- `cb edit`
- `cb remove`
- `cb delete`
- `cb doc`
- `cb tui`
- `cb-tui`
- `cb config`

Implemented command details:

- `init` supports one or more paths, `--path`, `--name`, repeated `--tag`, and `--no-prompt`.
- `list` supports `--json`, `--tag`, `--missing`, `--sort`, and `--order`.
- `search` supports metadata search and `--json`.
- `open` launches the selected project with configured editor resolution and updates `last_opened_at`.
- `show` supports human-readable metadata output and `--json`.
- `edit` supports metadata flags, full tag replacement, tag add/remove, and interactive editing.
- `remove` removes only the registry entry and supports `--yes`.
- `delete` moves projects to trash by default, supports guarded permanent delete, and cleans missing entries.
- `doc` supports resolved path output, raw output, rendered Markdown/text view, edit, and create.
- `tui` and `cb-tui` open the two-pane project browser with Docs/Tree preview tabs and editor handoff.
- `config` supports `get` and `set` for current global config keys.

## Remaining Commands

All v1 commands are implemented.

## Next Suggested Phase

Polish and harden the v1 experience:

1. Tag the first release after one final install/uninstall smoke on the target machine.
2. Add binary release artifacts if distributing beyond source installs.
3. Improve Markdown styling in the TUI if the current plain terminal rendering feels too sparse.

## Update Rule

Update this file after each milestone commit. Keep it aligned with `docs/code-base-v1-spec.md` so the remaining implementation work stays visible.
