use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use comfy_table::{Cell, Table, presets::UTF8_FULL};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, KeyCode, KeyEvent,
        KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use directories::ProjectDirs;
use ignore::WalkBuilder;
use inquire::{Confirm, Select, Text};
use pulldown_cmark::{Event as MarkdownEvent, Parser as MarkdownParser, Tag, TagEnd};
use rand::RngCore;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

mod commands;
mod storage;
mod tui;

use commands::{
    cmd_config, cmd_delete, cmd_doc, cmd_edit, cmd_init, cmd_list, cmd_open, cmd_remove,
    cmd_search, cmd_show,
};
use storage::{AppPaths, Config, NewProject, Project, ProjectChanges, Registry};
use tui::{
    cmd_tui, ensure_doc_exists_for_edit, ensure_project_available, is_markdown_path,
    render_markdown_to_terminal_text, resolve_editor_invocation, resolved_doc_path,
    spawn_editor_process, validate_delete_target, wait_for_editor,
};

const DEFAULT_DOC_PATH: &str = "README.md";
const PUBLIC_ID_RANDOM_BYTES: usize = 8;

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let paths = AppPaths::new()?;
    fs::create_dir_all(&paths.data_dir)?;
    let config = Config::load_or_default(&paths.config_file)?;
    let registry = Registry::open(&paths.db_file)?;

    match cli.command {
        Command::Init(args) => cmd_init(&registry, args),
        Command::List(args) => cmd_list(&registry, args),
        Command::Search(args) => cmd_search(&registry, args),
        Command::Open(args) => cmd_open(&registry, &config, args),
        Command::Show(args) => cmd_show(&registry, args),
        Command::Edit(args) => cmd_edit(&registry, args),
        Command::Remove(args) => cmd_remove(&registry, args),
        Command::Delete(args) => cmd_delete(&registry, args),
        Command::Doc(args) => cmd_doc(&registry, &config, args),
        Command::Tui => cmd_tui(&registry, config, paths.config_file),
        Command::Config(args) => cmd_config(paths.config_file, config, args),
    }
}

pub fn run_cb_tui() -> Result<()> {
    if cb_tui_help_requested() {
        println!(
            "Usage: cb-tui\n\nOptions:\n  -h, --help     Print help\n  -V, --version  Print version"
        );
        return Ok(());
    }
    if cb_tui_version_requested() {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let paths = AppPaths::new()?;
    fs::create_dir_all(&paths.data_dir)?;
    let config = Config::load_or_default(&paths.config_file)?;
    let registry = Registry::open(&paths.db_file)?;
    cmd_tui(&registry, config, paths.config_file)
}

#[cfg(test)]
fn is_cb_tui_binary_name(name: &str) -> bool {
    name == "cb-tui" || name == "cb-tui.exe"
}

#[cfg(test)]
mod tests;

fn cb_tui_help_requested() -> bool {
    env::args()
        .skip(1)
        .any(|arg| arg == "-h" || arg == "--help")
}

fn cb_tui_version_requested() -> bool {
    env::args()
        .skip(1)
        .any(|arg| arg == "-V" || arg == "--version")
}

#[derive(Debug, Parser)]
#[command(
    name = "cb",
    alias = "codebase",
    version,
    about = "Local project registry and launcher"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    List(ListArgs),
    Search(SearchArgs),
    Open(OpenArgs),
    Show(ShowArgs),
    Edit(EditArgs),
    Remove(RemoveArgs),
    Delete(DeleteArgs),
    Doc(DocArgs),
    Tui,
    Config(ConfigArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

    #[arg(long)]
    path: Option<PathBuf>,

    #[arg(long)]
    name: Option<String>,

    #[arg(long = "tag")]
    tags: Vec<String>,

    #[arg(long)]
    no_prompt: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long)]
    json: bool,

    #[arg(long)]
    tag: Option<String>,

    #[arg(long)]
    missing: bool,

    #[arg(long, value_enum, default_value_t = SortMode::Recent)]
    sort: SortMode,

    #[arg(long, value_enum, default_value_t = SortOrder::Desc)]
    order: SortOrder,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: String,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct OpenArgs {
    selector: String,

    #[arg(long = "with")]
    editor_override: Option<String>,
}

#[derive(Debug, Args)]
struct ShowArgs {
    selector: String,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct EditArgs {
    selector: String,

    #[arg(long)]
    name: Option<String>,

    #[arg(long = "doc")]
    doc_path: Option<String>,

    #[arg(long)]
    editor: Option<String>,

    #[arg(long)]
    editor_command: Option<String>,

    #[arg(long)]
    path: Option<PathBuf>,

    #[arg(long = "tag")]
    tags: Vec<String>,

    #[arg(long = "add-tag")]
    add_tags: Vec<String>,

    #[arg(long = "remove-tag")]
    remove_tags: Vec<String>,
}

#[derive(Debug, Args)]
struct RemoveArgs {
    selector: String,

    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct DeleteArgs {
    selector: String,

    #[arg(long)]
    yes: bool,

    #[arg(long)]
    permanent: bool,

    #[arg(long)]
    confirm_name: Option<String>,
}

#[derive(Debug, Args)]
struct DocArgs {
    selector: String,

    #[arg(long)]
    raw: bool,

    #[arg(long)]
    path: bool,

    #[arg(long)]
    edit: bool,

    #[arg(long)]
    create: bool,

    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Get { key: Option<String> },
    Set { key: String, value: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum SortMode {
    Recent,
    Name,
    Path,
    Created,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum SortOrder {
    Asc,
    Desc,
}

fn sort_projects(projects: &mut [Project], mode: SortMode, order: SortOrder) {
    projects.sort_by(|a, b| {
        a.missing
            .cmp(&b.missing)
            .then_with(|| {
                let ordering = match mode {
                    SortMode::Recent => a.last_opened_at.cmp(&b.last_opened_at),
                    SortMode::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortMode::Path => a.path.cmp(&b.path),
                    SortMode::Created => a.created_at.cmp(&b.created_at),
                };
                match order {
                    SortOrder::Asc => ordering,
                    SortOrder::Desc => ordering.reverse(),
                }
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
}

fn search_score(project: &Project, query_lower: &str) -> Option<u8> {
    let name = project.name.to_lowercase();
    let path = path_to_string(&project.path).to_lowercase();
    if name == query_lower {
        Some(0)
    } else if name.starts_with(query_lower) {
        Some(1)
    } else if name.contains(query_lower) {
        Some(2)
    } else if project.tags.iter().any(|tag| tag == query_lower) {
        Some(3)
    } else if path.contains(query_lower) {
        Some(4)
    } else {
        None
    }
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(path);
    expanded.canonicalize().with_context(|| {
        format!(
            "path does not exist or cannot be canonicalized: {}",
            path.display()
        )
    })
}

fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~"
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path.to_path_buf()
}

fn detect_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn validate_doc_path_soft(project_root: &Path, doc_path: &str, interactive: bool) -> Result<()> {
    let path = Path::new(doc_path);
    if path.is_absolute() {
        bail!("doc path must be relative to the project root");
    }
    if !is_text_doc_path(path) {
        bail!("doc path must be markdown or plain text");
    }
    let full_path = project_root.join(path);
    if !full_path.exists() && interactive {
        let save = Confirm::new(&format!(
            "Doc path does not exist: {}. Save anyway?",
            doc_path
        ))
        .with_default(true)
        .prompt()
        .context("doc path confirmation cancelled")?;
        if !save {
            bail!("doc path was not saved");
        }
    }
    Ok(())
}

fn is_text_doc_path(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md" | "markdown" | "txt") => true,
        Some(_) => false,
        None => true,
    }
}

fn validate_editor_name(editor: &str) -> Result<()> {
    if editor.trim().is_empty() {
        bail!("editor cannot be empty");
    }
    if editor.contains(char::is_whitespace) {
        bail!("editor names cannot contain spaces; use editor_command instead");
    }
    Ok(())
}

fn validate_editor_command(command: &str, interactive: bool) -> Result<()> {
    if command.trim().is_empty() {
        return Ok(());
    }
    if !command.contains("{path}") && interactive {
        let save = Confirm::new("Editor command does not contain {path}. Save anyway?")
            .with_default(false)
            .prompt()
            .context("editor command confirmation cancelled")?;
        if !save {
            bail!("editor command was not saved");
        }
    }
    Ok(())
}

fn normalize_tag(tag: &str) -> Result<String> {
    let normalized = tag.trim().to_lowercase();
    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        bail!("tag cannot be empty");
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        bail!("invalid tag '{tag}': tags must match [a-z0-9][a-z0-9_-]*");
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        bail!("invalid tag '{tag}': tags must match [a-z0-9][a-z0-9_-]*");
    }
    Ok(normalized)
}

fn dedup_tags(tags: &[String]) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    for tag in tags {
        seen.insert(normalize_tag(tag)?);
    }
    Ok(seen.into_iter().collect())
}

fn split_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn empty_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn display_path(path: &Path) -> String {
    let Ok(home) = env::var("HOME") else {
        return path.display().to_string();
    };
    let home = PathBuf::from(home);
    if let Ok(rest) = path.strip_prefix(&home) {
        if rest.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", rest.display())
        }
    } else {
        path.display().to_string()
    }
}

fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}

fn now_string() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.to_rfc3339()
}

fn is_interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn parse_sort_mode(value: &str) -> Result<SortMode> {
    match value.to_lowercase().as_str() {
        "recent" => Ok(SortMode::Recent),
        "name" => Ok(SortMode::Name),
        "path" => Ok(SortMode::Path),
        "created" => Ok(SortMode::Created),
        _ => bail!("invalid sort mode: {value}"),
    }
}

fn parse_sort_order(value: &str) -> Result<SortOrder> {
    match value.to_lowercase().as_str() {
        "asc" => Ok(SortOrder::Asc),
        "desc" => Ok(SortOrder::Desc),
        _ => bail!("invalid sort order: {value}"),
    }
}
