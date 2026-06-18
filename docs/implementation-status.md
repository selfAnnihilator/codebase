# Code Base Implementation Status

Baseline spec: [code-base-v1-spec.md](code-base-v1-spec.md)

Last reviewed commit: working tree after `ae18a48 Implement CLI registry core`

## Summary

Code Base currently has the non-TUI CLI workflows implemented. The project can be built as a Rust binary named `cb`, store project metadata in SQLite, manage TOML config, register/search/edit projects, open projects in an editor, view/edit/create project docs, and delete projects with guardrails.

The TUI, full tree preview, and richer terminal Markdown preview integration are not implemented yet.

## Status By Area

| Area | Status | Notes |
| --- | --- | --- |
| V1 spec | Done | Product decisions are captured in `docs/code-base-v1-spec.md`. |
| Rust crate | Done | Cargo binary crate exists with package `codebase` and binary target `cb`. |
| Dependency lockfile | Done | `Cargo.lock` is committed for reproducible app builds. |
| Config model | Done | TOML config defaults and `config get/set` are implemented. |
| SQLite registry | Done | Embedded schema creates `projects`, `tags`, and `project_tags`. |
| Project IDs | Done | Stable random `cb_...` IDs are generated with collision checks. |
| Path handling | Done | Registration canonicalizes existing paths and enforces unique stored paths. |
| Git detection | Done | Init-time containing Git root detection is stored and shown as an indicator. |
| Tags | Done | Tags are lowercase ASCII slugs with replacement/add/remove edit paths. |
| Search and selector resolution | Done | Metadata search and interactive ambiguity picker are reused by CLI workflows. |
| Table and JSON output | Done | `list`, `search`, and `show` support human/JSON output where planned. |
| Interactive prompts | Partial | CLI prompts exist for current commands; TUI interactions remain. |
| Editor resolution | Done | Project/global editor config, one-time overrides, direct execution, shell templates, and `{path}` substitution exist. |
| Documentation path metadata | Done | `doc_path` is stored, validated, resolved, viewed, edited, and created through `cb doc`. |
| Missing project handling | Partial | CLI open/doc/delete handle missing paths; TUI recovery states remain. |
| `open` command | Done | Launches configured editor, refuses missing paths, and updates `last_opened_at` after spawn. |
| `doc` command | Done | Supports path/raw/rendered view, edit, create, and parent directory creation. |
| `delete` command | Done | Supports trash-first deletion, permanent deletion guardrails, and missing-path cleanup. |
| TUI | Not started | Two-pane Ratatui interface remains. |
| Docs rendering | Partial | `cb doc` has basic terminal-native Markdown rendering; TUI rendering remains. |
| File tree preview | Not started | Live tree generation and ignore handling remain. |

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
- `config` supports `get` and `set` for current global config keys.

## Remaining Commands

These commands are specified but not implemented:

- `cb tui`

## Next Suggested Phase

Build the Ratatui TUI on top of the existing registry, selector, editor, docs, and future tree logic:

1. Add live file tree generation with ignore rules and truncation limits.
2. Add the two-pane TUI with search/list on the left and Docs/Tree tabs on the right.
3. Reuse existing project open and doc edit/create logic from the CLI workflows.

## Update Rule

Update this file after each milestone commit. Keep it aligned with `docs/code-base-v1-spec.md` so the remaining implementation work stays visible.
