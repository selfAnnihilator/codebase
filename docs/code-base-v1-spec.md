# Code Base V1 Spec

## Product Definition

Code Base is a local project registry and launcher. It lets users register existing directories, search them by name, path, and tags, preview their documentation and file tree in a TUI, and open projects or docs in a configured editor.

Code Base does not physically store projects. It stores local metadata about project directories that already exist on the machine.

## Product Boundary

V1 is local-only:

- No remote sync.
- No shared registry across machines.
- No background indexing.
- No GUI.
- No project scaffolding or templates.
- No moving project directories.
- No embedded text editor.
- No Git history or live Git status.

The only Git feature in v1 is detecting and storing the Git root during project registration, then showing a Git indicator in project lists.

## Naming

- Primary command: `cb`
- Full alias: `codebase`
- App display name: `Code Base`

The same binary should be installable under both `cb` and `codebase`. Behavior is identical for both command names.

## Technology

V1 should be implemented in Rust.

Recommended stack:

- CLI: `clap`
- TUI: `ratatui` + `crossterm`
- SQLite: `rusqlite`
- Config: `serde` + `toml`
- OS directories: `directories`
- File tree walking: `ignore`
- Trash deletion: `trash`
- Markdown rendering: existing Ratatui-compatible renderer if viable; otherwise `pulldown-cmark` with a scoped terminal renderer

Use synchronous SQLite and filesystem operations in v1.

## Storage

Use SQLite for the project registry and TOML for user configuration.

Registry database:

- Linux: `~/.local/share/codebase/codebase.db`
- macOS: app data directory equivalent
- Windows: `%APPDATA%\Codebase\codebase.db`

Config file:

- Linux: `~/.config/codebase/config.toml`
- macOS: app config directory equivalent
- Windows: `%APPDATA%\Codebase\config.toml`

SQLite migrations should be embedded in the binary.

## SQLite Schema

Initial schema:

```sql
CREATE TABLE projects (
  id INTEGER PRIMARY KEY,
  public_id TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  git_root TEXT,
  doc_path TEXT NOT NULL DEFAULT 'README.md',
  editor TEXT,
  editor_command TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_opened_at TEXT
);

CREATE TABLE tags (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE project_tags (
  project_id INTEGER NOT NULL,
  tag_id INTEGER NOT NULL,
  PRIMARY KEY (project_id, tag_id),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
```

Public project IDs:

- Format: `cb_` plus random hex.
- Use 6-byte or 8-byte random values encoded as hex.
- Generate once during registration.
- Never change when name or path changes.
- Collision-check before saving.
- Do not derive IDs from name or path.

## Path Rules

On registration:

- Path must exist.
- Expand `~`.
- Resolve relative paths against the current working directory.
- Canonicalize and store an absolute normalized path.
- Enforce uniqueness on canonical path.

After registration:

- If the directory disappears, keep the last known canonical path.
- Mark the project as missing in list/search/TUI.
- Opening a missing project is refused with recovery instructions.
- Editing a missing project's path requires the new path to exist and be canonicalized before saving.

## Config

Example:

```toml
editor = "nvim"
editor_command = ""

[tree]
max_depth = 4
max_entries = 500
show_hidden = false
respect_gitignore = true

[tui]
sort_mode = "recent"
sort_order = "desc"
```

`editor_command` is optional. If set, it overrides `editor` according to the editor resolution rules.

## Project Metadata

Each project stores:

- `public_id`
- `name`
- canonical absolute `path`
- optional detected `git_root`
- `doc_path`
- optional per-project `editor`
- optional per-project `editor_command`
- `created_at`
- `updated_at`
- optional `last_opened_at`
- tags

No separate notes field in v1. Project overview content belongs in the configured documentation file.

## Tags

Tags are included in v1.

Rules:

- Store tags lowercase.
- Match tags case-insensitively.
- Preserve project name casing, but project matching is case-insensitive.
- Tags cannot contain spaces.
- Allowed tag pattern: `[a-z0-9][a-z0-9_-]*`

Examples:

- `work`
- `backend`
- `client-a`
- `client_a`
- `v2`

## Project Registration

Commands:

```bash
cb init
cb init ~/work/api
cb init --path ~/work/api
cb init ~/a ~/b ~/c
```

`cb init` should be interactive by default and also support flags.

Interactive single-project flow:

```text
Path: /canonical/path
Name [folder-name]:
Tags []:
Doc path [README.md]:
Git: /detected/git/root or none
Register? [Y/n]
```

Multi-project flow should ask compactly per project:

```text
Project 1 of 3
Path: ~/a
Name [a]:
Tags []:
Doc path [README.md]:
Register? [Y/n]
```

Flags:

```bash
cb init --name "API Server" --tag work --tag backend
cb init --name "API Server" --no-prompt
```

If `--name` is provided with multiple paths, reject it in v1.

For multiple paths:

- Continue if one path fails.
- Show a summary of registered and skipped paths.
- Exit `0` if all succeeded.
- Exit `1` if some succeeded and some failed or were skipped.
- Exit `2` if none succeeded.

If a path is already registered, do not create duplicates. Show the existing entry and offer edit actions.

## Git Detection

During init:

- Register the directory the user selected, regardless of whether it is inside a larger Git repo.
- Detect whichever Git repo the selected directory belongs to.
- Store that detected Git root.

After init:

- Do not refresh Git detection automatically in v1.
- Show only a Git indicator in lists/TUI.
- Do not show Git history or live status in v1.

## Search And Matching

Search should include:

- Project names
- Paths
- Tags

Do not search documentation contents in v1.

Matching is case-insensitive for names, paths, and tags. Preserve original project name casing for display.

Ranking:

1. Exact name match
2. Prefix name match
3. Fuzzy name match
4. Tag match
5. Path match

Duplicate project names are allowed. Duplicate canonical paths are not.

When selectors are ambiguous:

- Interactive commands may show a picker when attached to a terminal.
- Non-interactive commands fail and show candidates.
- Commands using `--yes` must not show a picker.

## CLI Commands

V1 commands:

```bash
cb init
cb list
cb search <query>
cb open <query-or-id>
cb show <query-or-id>
cb edit <query-or-id>
cb remove <query-or-id>
cb delete <query-or-id>
cb doc <query-or-id>
cb tui
cb config
```

### list

Default output is a plain table:

```text
ID           Name          Path                  Tags        Git  Status
cb_a3f91c7e  API Server    ~/work/api-server     work,api    yes  available
cb_b92d10aa  Notes Site    ~/sites/notes         personal    no   missing
```

Flags:

```bash
cb list --json
cb list --tag work
cb list --missing
cb list --sort recent --order desc
```

Missing projects are included by default and marked as missing.

### search

Searches names, paths, and tags.

Output should match `list` table style, with `--json` support.

### open

Examples:

```bash
cb open api
cb open cb_a3f91c7e
cb open api --with code
cb open api --with "tmux new-window -c {path} nvim ."
```

Behavior:

- Uses the same metadata search as `search`.
- Opens directly only for a unique match.
- Shows picker for ambiguous matches in interactive mode.
- Fails with candidates in non-interactive mode.
- Refuses to open missing paths.
- Updates `last_opened_at` immediately after successfully spawning the editor command.
- Waits for the editor command to exit.

### show

Shows metadata only by default:

```text
ID: cb_a3f91c7e
Name: API Server
Path: /home/abhi/work/api
Status: available
Git: /home/abhi/work/api
Doc: README.md
Tags: work, backend
Editor: nvim
Editor command:
Created: 2026-06-18 11:20
Last opened: 2026-06-18 12:04
```

Support:

```bash
cb show api --json
```

Do not include documentation preview content by default.

### edit

Interactive by default, flags for direct edits.

Interactive fields:

```text
Name: API Server
Path: /home/abhi/work/api
Tags: work, backend
Doc path: README.md
Editor: nvim
Editor command:
```

Flags:

```bash
cb edit api --name "API Server"
cb edit api --doc docs/overview.md
cb edit api --editor code
cb edit api --editor-command "tmux new-window -c {path} nvim ."
cb edit api --path ~/new/api
cb edit api --tag work --tag backend
cb edit api --add-tag oss
cb edit api --remove-tag old
```

Tag semantics:

- `--tag` replaces the full tag set.
- `--add-tag` and `--remove-tag` are incremental.

### remove

Removes only the registry entry from SQLite. It never deletes files from disk.

Example confirmation:

```text
Remove "API Server" from Code Base?
Path: ~/work/api-server

This will not delete project files.
[remove] [cancel]
```

### delete

Deletes the project directory from disk and removes the registry entry.

Rules:

- Default delete moves the project to trash/recycle bin.
- Permanent delete requires `--permanent`.
- Interactive delete to trash requires typing the project name.
- Non-interactive delete to trash can use `--yes`.
- `--yes` is not enough for permanent delete.
- Permanent delete requires stronger confirmation such as `--confirm-name "API Server"`.
- Refuse dangerous paths such as `/`, home directory, and shallow system paths.
- If the path is missing, ask to remove only the registry entry.
- Ambiguous selectors may show a picker only in interactive mode, never with `--yes`.

Examples:

```bash
cb delete api
cb delete api --yes
cb delete api --permanent --confirm-name "API Server"
```

### doc

Views or edits the configured project documentation file.

Commands:

```bash
cb doc api
cb doc api --raw
cb doc api --path
cb doc api --edit
cb doc api --create
cb doc api --create --yes
```

Behavior:

- Default view renders the configured doc file to the terminal.
- `--raw` prints raw file content.
- `--path` prints the resolved documentation path.
- `--edit` opens the existing doc in the configured editor.
- If the doc is missing, `--edit` asks whether to create it.
- `--create` creates the doc if missing, then opens it.
- If `--create` is used and the doc already exists, do not overwrite; print a note and open the existing file.
- Creating parent directories is allowed after confirmation.
- Non-interactive file creation requires `--yes`.

Supported doc formats in v1:

- Markdown: `.md`, `.markdown`
- Plain text: `.txt` and extensionless text-like files

Do not support other formats in v1.

## Documentation Path

Each project has a `doc_path`.

Default:

```text
README.md
```

Rules:

- Store relative to the project root.
- Soft-validate when setting.
- Must point to Markdown or text.
- If the file exists, accept.
- If the file does not exist, warn and ask whether to save anyway.
- Missing docs are shown as a recoverable state in the TUI.

Code Base must not create documentation automatically during init.

## Editor Resolution

Default editor is `nvim`.

Resolution order:

1. One-time `--with`, if provided.
2. Project `editor_command`, if set.
3. Global `editor_command`, if set.
4. Project `editor`, if set.
5. Global `editor`, if set.
6. `nvim`.

For project opening, `{path}` means the project path.

For doc editing, `{path}` means the documentation file path.

V1 supports only `{path}` as a template variable.

Future versions may add explicit variables like `{project_path}` and `{doc_path}`.

Simple editor names:

- Execute directly with `Command::new(editor).arg(path)`.
- Do not execute through a shell.
- Values with spaces are invalid; users should use `editor_command`.

Custom `editor_command`:

- Execute through the platform shell.
- Replace only literal `{path}`.
- Quote/escape `{path}` for the current shell.
- Warn if the command does not contain `{path}`.
- Allow saving after confirmation.

Named presets may include:

- `nvim`
- `code`
- `cursor`
- `zed`

## TUI

V1 TUI is first-class. GUI is deferred.

Launch:

```bash
cb tui
```

Main layout:

- Two vertical panes.
- Left pane: search bar at top, project list below.
- Right pane: preview area with two tabs: `Docs` and `Tree`.
- Bottom status bar similar in spirit to Neovim.

Left pane:

- Search is focused when TUI opens.
- Search filters live as the user types.
- Search matches name, path, and tags.
- Results show name, path, tags, Git indicator, and missing indicator.

Right pane:

- `Docs` tab previews the configured project documentation.
- `Tree` tab previews the file structure.
- Tabs can be selected by keyboard or mouse click.

Status bar should show:

- Current sort mode
- Sort order
- Project count
- Useful key hints

Example:

```text
Sort: Recent desc | 42 projects | Ctrl+r mode | Ctrl+o order | Enter open
```

## TUI Keymap

Required v1 keybindings:

- `Enter`: open selected project.
- `Tab`: cycle focus between left pane and right pane.
- `/`: focus search from anywhere.
- `Esc`: clear search if non-empty; otherwise move preview focus back to left pane; otherwise quit.
- `1`: switch to `Docs` tab.
- `2`: switch to `Tree` tab.
- Mouse click on `Docs` or `Tree`: switch tab.
- Arrow keys: operate on the focused pane.
- PageUp/PageDown: scroll focused preview pane where applicable.
- Mouse wheel: scroll pane under cursor when terminal mouse support is available.
- `Ctrl+r`: cycle sort mode.
- `Ctrl+o`: toggle sort order.
- `e`: edit selected project metadata.
- `d`: edit/open selected project documentation.

Preview scrolling:

- Docs/tree scroll independently of the project list.
- Reset scroll when selected project changes.
- Preserve scroll when switching between Docs and Tree for the same project.

## TUI Sorting

Sort modes:

- `recent`
- `name`
- `path`
- `created`

Sort order:

- `asc`
- `desc`

Keybindings:

- `Ctrl+r`: cycle sort mode.
- `Ctrl+o`: toggle sort order.

Persist sort mode/order globally in TOML config.

Missing projects always sort below available projects in all modes.

## TUI States

The TUI should show recoverable states inline instead of crashing or blanking:

- Missing project path.
- Missing documentation file.
- Permission denied.
- Documentation too large/truncated.
- Tree too large/truncated.

Missing doc actions:

- Choose/change doc path.
- Create doc.
- Edit project.

Missing project actions:

- Relocate via edit.
- Remove registry entry.

## Documentation Rendering

Markdown rendering should be terminal-native, not pixel-perfect browser Markdown.

Support in v1:

- Heading hierarchy styling.
- Bold, italic, and inline code styling where terminal support allows.
- Fenced code blocks with visible block styling.
- Blockquotes.
- Lists and nested lists.
- Tables if the chosen renderer supports them reasonably.
- Links visible as text plus URL when useful.
- Wrapping and scrolling.

Not required in v1:

- Images.
- HTML rendering.
- Mermaid diagrams.
- GitHub task checkbox interactivity.
- Exact GitHub CSS layout.

Use an existing Rust Markdown-to-terminal renderer if it fits Ratatui cleanly. Otherwise, use `pulldown-cmark` with a scoped renderer.

Plain text docs should render as wrapped text.

## File Tree Preview

Tree preview is read live from disk. Do not store tree snapshots in SQLite.

Defaults:

```toml
[tree]
max_depth = 4
max_entries = 500
show_hidden = false
respect_gitignore = true
```

Rules:

- Respect `.gitignore` when possible.
- Always skip `.git`.
- Skip common heavy folders such as `node_modules`, `target`, `dist`, `build`, `.venv`, and `__pycache__`.
- Hide dotfiles by default except important docs such as `.env.example`.
- Show truncation markers when limits are reached.

These defaults should be globally configurable in v1.

## Derived Data

Read these live from disk on demand:

- Documentation preview content.
- File tree.
- Missing path status.
- Missing doc status.

Do not store README contents, rendered docs, file tree snapshots, or Git status in SQLite.

## V1 Exclusions

V1 explicitly excludes:

- GUI.
- Remote sync.
- Git history.
- Live Git status.
- Background indexing.
- Full-text search of documentation.
- Embedded text editor.
- Project templates or scaffolding.
- Moving project directories.
- Multiple machines/shared registry.
- Browser-style Markdown rendering with images, HTML, Mermaid, or exact CSS layout.
