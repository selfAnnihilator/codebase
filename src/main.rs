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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

const DEFAULT_DOC_PATH: &str = "README.md";
const PUBLIC_ID_RANDOM_BYTES: usize = 8;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if invoked_as_cb_tui() {
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
        return cmd_tui(&registry, config, paths.config_file);
    }

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

fn invoked_as_cb_tui() -> bool {
    env::args_os()
        .next()
        .and_then(|arg| PathBuf::from(arg).file_name().map(|name| name.to_owned()))
        .and_then(|name| name.to_str().map(ToOwned::to_owned))
        .is_some_and(|name| is_cb_tui_binary_name(&name))
}

fn is_cb_tui_binary_name(name: &str) -> bool {
    name == "cb-tui" || name == "cb-tui.exe"
}

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

#[derive(Debug)]
struct AppPaths {
    data_dir: PathBuf,
    config_file: PathBuf,
    db_file: PathBuf,
}

impl AppPaths {
    fn new() -> Result<Self> {
        let project_dirs = ProjectDirs::from("", "", "codebase")
            .ok_or_else(|| anyhow!("could not determine OS app directories"))?;
        let data_dir = env::var_os("CODEBASE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| project_dirs.data_dir().to_path_buf());
        let config_dir = env::var_os("CODEBASE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| project_dirs.config_dir().to_path_buf());
        Ok(Self {
            db_file: data_dir.join("codebase.db"),
            data_dir,
            config_file: config_dir.join("config.toml"),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    editor: String,
    #[serde(default)]
    editor_command: String,
    #[serde(default)]
    tree: TreeConfig,
    #[serde(default)]
    tui: TuiConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TreeConfig {
    max_depth: usize,
    max_entries: usize,
    show_hidden: bool,
    respect_gitignore: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TuiConfig {
    sort_mode: SortMode,
    sort_order: SortOrder,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: "nvim".to_string(),
            editor_command: String::new(),
            tree: TreeConfig::default(),
            tui: TuiConfig::default(),
        }
    }
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_entries: 500,
            show_hidden: false,
            respect_gitignore: true,
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            sort_mode: SortMode::Recent,
            sort_order: SortOrder::Desc,
        }
    }
}

impl Config {
    fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse config {}", path.display()))
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
struct Project {
    id: i64,
    public_id: String,
    name: String,
    path: PathBuf,
    git_root: Option<PathBuf>,
    doc_path: String,
    editor: Option<String>,
    editor_command: Option<String>,
    created_at: String,
    updated_at: String,
    last_opened_at: Option<String>,
    tags: Vec<String>,
    missing: bool,
}

#[derive(Debug)]
struct NewProject {
    name: String,
    path: PathBuf,
    git_root: Option<PathBuf>,
    doc_path: String,
    tags: Vec<String>,
}

#[derive(Debug)]
struct ProjectChanges {
    name: Option<String>,
    path: Option<PathBuf>,
    doc_path: Option<String>,
    editor: Option<Option<String>>,
    editor_command: Option<Option<String>>,
    replace_tags: Option<Vec<String>>,
    add_tags: Vec<String>,
    remove_tags: Vec<String>,
}

struct Registry {
    conn: Connection,
}

impl Registry {
    fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let registry = Self { conn };
        registry.migrate()?;
        Ok(registry)
    }

    #[cfg(test)]
    fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let registry = Self { conn };
        registry.migrate()?;
        Ok(registry)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
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

            CREATE TABLE IF NOT EXISTS tags (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS project_tags (
              project_id INTEGER NOT NULL,
              tag_id INTEGER NOT NULL,
              PRIMARY KEY (project_id, tag_id),
              FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
              FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            );
            "#,
        )?;
        Ok(())
    }

    fn insert_project(&self, project: NewProject) -> Result<Project> {
        if self.project_by_path(&project.path)?.is_some() {
            bail!("path is already registered: {}", project.path.display());
        }

        let now = now_string();
        let public_id = self.generate_public_id()?;
        self.conn.execute(
            r#"
            INSERT INTO projects
                (public_id, name, path, git_root, doc_path, created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                public_id,
                project.name,
                path_to_string(&project.path),
                project.git_root.as_ref().map(|p| path_to_string(p)),
                project.doc_path,
                now,
                now,
            ],
        )?;

        let id = self.conn.last_insert_rowid();
        self.set_tags(id, &project.tags)?;
        self.project_by_id(id)?
            .ok_or_else(|| anyhow!("inserted project was not found"))
    }

    fn generate_public_id(&self) -> Result<String> {
        for _ in 0..32 {
            let mut bytes = [0_u8; PUBLIC_ID_RANDOM_BYTES];
            rand::rng().fill_bytes(&mut bytes);
            let candidate = format!(
                "cb_{}",
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
            let exists: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM projects WHERE public_id = ?1",
                    [&candidate],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Ok(candidate);
            }
        }
        bail!("failed to generate unique public id")
    }

    fn project_by_id(&self, id: i64) -> Result<Option<Project>> {
        let mut projects = self.projects_where("p.id = ?1", params![id])?;
        Ok(projects.pop())
    }

    fn project_by_path(&self, path: &Path) -> Result<Option<Project>> {
        let mut projects = self.projects_where("p.path = ?1", params![path_to_string(path)])?;
        Ok(projects.pop())
    }

    fn all_projects(&self, args: ListArgs) -> Result<Vec<Project>> {
        let mut projects = if let Some(tag) = args.tag {
            let tag = normalize_tag(&tag)?;
            self.projects_with_tag(&tag)?
        } else {
            self.projects_where("1 = 1", [])?
        };
        sort_projects(&mut projects, args.sort, args.order);
        Ok(projects)
    }

    fn search(&self, query: &str) -> Result<Vec<Project>> {
        let query_lower = query.to_lowercase();
        let mut scored = self
            .projects_where("1 = 1", [])?
            .into_iter()
            .filter_map(|project| {
                search_score(&project, &query_lower).map(|score| (score, project))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|(a_score, a), (b_score, b)| {
            a_score
                .cmp(b_score)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.path.cmp(&b.path))
        });
        Ok(scored.into_iter().map(|(_, project)| project).collect())
    }

    fn resolve_selector(&self, selector: &str, allow_picker: bool) -> Result<Project> {
        if selector.starts_with("cb_") {
            let matches = self.projects_where("p.public_id = ?1", params![selector])?;
            if let Some(project) = matches.into_iter().next() {
                return Ok(project);
            }
        }

        let matches = self.search(selector)?;
        match matches.len() {
            0 => bail!("no project matches '{selector}'"),
            1 => Ok(matches.into_iter().next().expect("one match")),
            _ if allow_picker => {
                let options = matches
                    .iter()
                    .map(|project| {
                        format!("{}  {}", project.public_id, display_path(&project.path))
                    })
                    .collect::<Vec<_>>();
                let selected = Select::new("Multiple projects match. Choose one:", options)
                    .prompt()
                    .context("project selection cancelled")?;
                let public_id = selected
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| anyhow!("invalid selected project"))?;
                matches
                    .into_iter()
                    .find(|project| project.public_id == public_id)
                    .ok_or_else(|| anyhow!("selected project was not found"))
            }
            _ => {
                let candidates = matches
                    .iter()
                    .map(|project| {
                        format!(
                            "{} {} {}",
                            project.public_id,
                            project.name,
                            display_path(&project.path)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                bail!("selector '{selector}' is ambiguous:\n{candidates}");
            }
        }
    }

    fn update_project(&self, id: i64, changes: ProjectChanges) -> Result<Project> {
        let Some(mut project) = self.project_by_id(id)? else {
            bail!("project not found");
        };

        let name = changes.name.unwrap_or(project.name);
        let path = changes.path.unwrap_or(project.path);
        let doc_path = changes.doc_path.unwrap_or(project.doc_path);
        let editor = changes.editor.unwrap_or(project.editor);
        let editor_command = changes.editor_command.unwrap_or(project.editor_command);
        let now = now_string();

        self.conn.execute(
            r#"
            UPDATE projects
            SET name = ?1,
                path = ?2,
                doc_path = ?3,
                editor = ?4,
                editor_command = ?5,
                updated_at = ?6
            WHERE id = ?7
            "#,
            params![
                name,
                path_to_string(&path),
                doc_path,
                editor,
                editor_command,
                now,
                id
            ],
        )?;

        if let Some(tags) = changes.replace_tags {
            self.set_tags(id, &tags)?;
        }
        for tag in changes.add_tags {
            self.add_tag(id, &tag)?;
        }
        for tag in changes.remove_tags {
            self.remove_tag(id, &tag)?;
        }

        project = self
            .project_by_id(id)?
            .ok_or_else(|| anyhow!("project not found"))?;
        Ok(project)
    }

    fn remove_project(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    fn touch_last_opened(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE projects SET last_opened_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now_string(), id],
        )?;
        Ok(())
    }

    fn set_tags(&self, project_id: i64, tags: &[String]) -> Result<()> {
        self.conn.execute(
            "DELETE FROM project_tags WHERE project_id = ?1",
            [project_id],
        )?;
        for tag in dedup_tags(tags)? {
            self.add_tag(project_id, &tag)?;
        }
        Ok(())
    }

    fn add_tag(&self, project_id: i64, tag: &str) -> Result<()> {
        let tag = normalize_tag(tag)?;
        self.conn
            .execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [&tag])?;
        let tag_id: i64 =
            self.conn
                .query_row("SELECT id FROM tags WHERE name = ?1", [&tag], |row| {
                    row.get(0)
                })?;
        self.conn.execute(
            "INSERT OR IGNORE INTO project_tags (project_id, tag_id) VALUES (?1, ?2)",
            params![project_id, tag_id],
        )?;
        Ok(())
    }

    fn remove_tag(&self, project_id: i64, tag: &str) -> Result<()> {
        let tag = normalize_tag(tag)?;
        self.conn.execute(
            r#"
            DELETE FROM project_tags
            WHERE project_id = ?1
              AND tag_id IN (SELECT id FROM tags WHERE name = ?2)
            "#,
            params![project_id, tag],
        )?;
        Ok(())
    }

    fn projects_with_tag(&self, tag: &str) -> Result<Vec<Project>> {
        self.projects_where(
            "p.id IN (
                SELECT pt.project_id
                FROM project_tags pt
                JOIN tags t ON t.id = pt.tag_id
                WHERE t.name = ?1
            )",
            params![tag],
        )
    }

    fn projects_where<P>(&self, where_sql: &str, params: P) -> Result<Vec<Project>>
    where
        P: rusqlite::Params,
    {
        let sql = format!(
            r#"
            SELECT
                p.id,
                p.public_id,
                p.name,
                p.path,
                p.git_root,
                p.doc_path,
                p.editor,
                p.editor_command,
                p.created_at,
                p.updated_at,
                p.last_opened_at,
                COALESCE(GROUP_CONCAT(t.name, ','), '') AS tags
            FROM projects p
            LEFT JOIN project_tags pt ON pt.project_id = p.id
            LEFT JOIN tags t ON t.id = pt.tag_id
            WHERE {where_sql}
            GROUP BY p.id
            ORDER BY p.name COLLATE NOCASE ASC, p.path ASC
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params, |row| {
            let path: String = row.get(3)?;
            let git_root: Option<String> = row.get(4)?;
            let tags: String = row.get(11)?;
            let path = PathBuf::from(path);
            let missing = !path.exists();
            Ok(Project {
                id: row.get(0)?,
                public_id: row.get(1)?,
                name: row.get(2)?,
                path,
                git_root: git_root.map(PathBuf::from),
                doc_path: row.get(5)?,
                editor: row.get(6)?,
                editor_command: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                last_opened_at: row.get(10)?,
                tags: split_tags(&tags),
                missing,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

fn cmd_init(registry: &Registry, mut args: InitArgs) -> Result<()> {
    let mut paths = std::mem::take(&mut args.paths);
    if let Some(path) = args.path.take() {
        paths.push(path);
    }
    if paths.is_empty() {
        paths.push(env::current_dir()?);
    }
    if paths.len() > 1 && args.name.is_some() {
        bail!("--name can only be used with a single path");
    }

    let mut registered = Vec::new();
    let mut skipped = Vec::new();
    for path in paths {
        match prepare_init_project(&path, &args) {
            Ok(Some(new_project)) => match registry.insert_project(new_project) {
                Ok(project) => registered.push(project),
                Err(error) => skipped.push((path, error.to_string())),
            },
            Ok(None) => skipped.push((path, "cancelled".to_string())),
            Err(error) => skipped.push((path, error.to_string())),
        }
    }

    if !registered.is_empty() {
        println!("Registered:");
        for project in &registered {
            println!("- {}  {}", project.name, display_path(&project.path));
        }
    }
    if !skipped.is_empty() {
        println!("Skipped:");
        for (path, reason) in &skipped {
            println!("- {}  {}", path.display(), reason);
        }
    }

    if registered.is_empty() {
        std::process::exit(2);
    }
    if !skipped.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

fn prepare_init_project(path: &Path, args: &InitArgs) -> Result<Option<NewProject>> {
    let canonical = canonicalize_existing(path)?;
    let suggested_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string();
    let git_root = detect_git_root(&canonical);

    let mut name = args.name.clone().unwrap_or(suggested_name);
    let mut tags = dedup_tags(&args.tags)?;
    let mut doc_path = DEFAULT_DOC_PATH.to_string();

    if !args.no_prompt && is_interactive() {
        println!("Path: {}", canonical.display());
        println!(
            "Git: {}",
            git_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        name = Text::new("Name:")
            .with_default(&name)
            .prompt()
            .context("name prompt cancelled")?;
        let tags_raw = Text::new("Tags:")
            .with_help_message("space-separated lowercase tags")
            .with_default(&tags.join(" "))
            .prompt()
            .context("tags prompt cancelled")?;
        tags = dedup_tags(
            &tags_raw
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>(),
        )?;
        doc_path = Text::new("Doc path:")
            .with_default(DEFAULT_DOC_PATH)
            .prompt()
            .context("doc path prompt cancelled")?;
        if !Confirm::new("Register?")
            .with_default(true)
            .prompt()
            .context("confirm prompt cancelled")?
        {
            return Ok(None);
        }
    }

    validate_doc_path_soft(&canonical, &doc_path, !args.no_prompt && is_interactive())?;

    Ok(Some(NewProject {
        name,
        path: canonical,
        git_root,
        doc_path,
        tags,
    }))
}

fn cmd_list(registry: &Registry, args: ListArgs) -> Result<()> {
    let json = args.json;
    let projects = registry.all_projects(args)?;
    print_projects(projects, json)
}

fn cmd_search(registry: &Registry, args: SearchArgs) -> Result<()> {
    let projects = registry.search(&args.query)?;
    print_projects(projects, args.json)
}

fn cmd_open(registry: &Registry, config: &Config, args: OpenArgs) -> Result<()> {
    let project = registry.resolve_selector(&args.selector, is_interactive())?;
    ensure_project_available(&project)?;
    let invocation =
        resolve_editor_invocation(config, &project, args.editor_override, &project.path)?;
    let mut child = spawn_editor_process(&invocation)?;
    registry.touch_last_opened(project.id)?;
    wait_for_editor(&mut child)?;
    Ok(())
}

fn cmd_show(registry: &Registry, args: ShowArgs) -> Result<()> {
    let project = registry.resolve_selector(&args.selector, is_interactive())?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&project)?);
        return Ok(());
    }

    println!("ID: {}", project.public_id);
    println!("Name: {}", project.name);
    println!("Path: {}", project.path.display());
    println!(
        "Status: {}",
        if project.missing {
            "missing"
        } else {
            "available"
        }
    );
    println!(
        "Git: {}",
        project
            .git_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("Doc: {}", project.doc_path);
    println!("Tags: {}", project.tags.join(", "));
    println!("Editor: {}", project.editor.as_deref().unwrap_or(""));
    println!(
        "Editor command: {}",
        project.editor_command.as_deref().unwrap_or("")
    );
    println!("Created: {}", project.created_at);
    println!(
        "Last opened: {}",
        project.last_opened_at.as_deref().unwrap_or("never")
    );
    Ok(())
}

fn cmd_edit(registry: &Registry, args: EditArgs) -> Result<()> {
    let project = registry.resolve_selector(&args.selector, is_interactive())?;
    let has_direct_flags = args.name.is_some()
        || args.doc_path.is_some()
        || args.editor.is_some()
        || args.editor_command.is_some()
        || args.path.is_some()
        || !args.tags.is_empty()
        || !args.add_tags.is_empty()
        || !args.remove_tags.is_empty();

    let changes = if has_direct_flags {
        changes_from_edit_args(&project, args)?
    } else if is_interactive() {
        prompt_project_changes(&project)?
    } else {
        bail!("no edit flags provided and terminal is not interactive");
    };

    let updated = registry.update_project(project.id, changes)?;
    println!("Updated {} ({})", updated.name, updated.public_id);
    Ok(())
}

fn changes_from_edit_args(project: &Project, args: EditArgs) -> Result<ProjectChanges> {
    let path = args
        .path
        .map(|path| canonicalize_existing(&path))
        .transpose()?;
    if let Some(doc_path) = &args.doc_path {
        validate_doc_path_soft(&project.path, doc_path, is_interactive())?;
    }
    if let Some(command) = &args.editor_command {
        validate_editor_command(command, is_interactive())?;
    }
    let replace_tags = if args.tags.is_empty() {
        None
    } else {
        Some(dedup_tags(&args.tags)?)
    };
    Ok(ProjectChanges {
        name: args.name,
        path,
        doc_path: args.doc_path,
        editor: args.editor.map(Some),
        editor_command: args.editor_command.map(Some),
        replace_tags,
        add_tags: dedup_tags(&args.add_tags)?,
        remove_tags: dedup_tags(&args.remove_tags)?,
    })
}

fn prompt_project_changes(project: &Project) -> Result<ProjectChanges> {
    let name = Text::new("Name:")
        .with_default(&project.name)
        .prompt()
        .context("name prompt cancelled")?;
    let path_raw = Text::new("Path:")
        .with_default(&path_to_string(&project.path))
        .prompt()
        .context("path prompt cancelled")?;
    let doc_path = Text::new("Doc path:")
        .with_default(&project.doc_path)
        .prompt()
        .context("doc path prompt cancelled")?;
    let editor = Text::new("Editor:")
        .with_default(project.editor.as_deref().unwrap_or(""))
        .prompt()
        .context("editor prompt cancelled")?;
    let editor_command = Text::new("Editor command:")
        .with_default(project.editor_command.as_deref().unwrap_or(""))
        .prompt()
        .context("editor command prompt cancelled")?;
    let tags_raw = Text::new("Tags:")
        .with_default(&project.tags.join(" "))
        .prompt()
        .context("tags prompt cancelled")?;

    let path = canonicalize_existing(Path::new(&path_raw))?;
    validate_doc_path_soft(&path, &doc_path, true)?;
    if !editor_command.is_empty() {
        validate_editor_command(&editor_command, true)?;
    }

    Ok(ProjectChanges {
        name: Some(name),
        path: Some(path),
        doc_path: Some(doc_path),
        editor: Some(empty_to_none(editor)),
        editor_command: Some(empty_to_none(editor_command)),
        replace_tags: Some(dedup_tags(
            &tags_raw
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>(),
        )?),
        add_tags: Vec::new(),
        remove_tags: Vec::new(),
    })
}

fn cmd_remove(registry: &Registry, args: RemoveArgs) -> Result<()> {
    let project = registry.resolve_selector(&args.selector, is_interactive() && !args.yes)?;
    if !args.yes {
        if !is_interactive() {
            bail!("remove requires --yes when terminal is not interactive");
        }
        let confirmed = Confirm::new(&format!(
            "Remove '{}' from Code Base? This will not delete project files.",
            project.name
        ))
        .with_default(false)
        .prompt()
        .context("remove confirmation cancelled")?;
        if !confirmed {
            println!("Cancelled");
            return Ok(());
        }
    }
    registry.remove_project(project.id)?;
    println!("Removed {} ({})", project.name, project.public_id);
    Ok(())
}

fn cmd_delete(registry: &Registry, args: DeleteArgs) -> Result<()> {
    let project = registry.resolve_selector(&args.selector, is_interactive() && !args.yes)?;

    if project.missing {
        if !args.yes {
            if !is_interactive() {
                bail!(
                    "delete for missing projects requires --yes when terminal is not interactive"
                );
            }
            let confirmed = Confirm::new(&format!(
                "Project path is missing: {}. Remove registry entry only?",
                project.path.display()
            ))
            .with_default(false)
            .prompt()
            .context("missing-path delete confirmation cancelled")?;
            if !confirmed {
                println!("Cancelled");
                return Ok(());
            }
        }
        registry.remove_project(project.id)?;
        println!(
            "Removed missing project entry {} ({})",
            project.name, project.public_id
        );
        return Ok(());
    }

    validate_delete_target(&project.path)?;
    if args.permanent {
        let Some(confirm_name) = args.confirm_name.as_deref() else {
            bail!("permanent delete requires --confirm-name <project name>");
        };
        if confirm_name != project.name {
            bail!(
                "--confirm-name must exactly match project name '{}'",
                project.name
            );
        }
        fs::remove_dir_all(&project.path)
            .with_context(|| format!("failed to permanently delete {}", project.path.display()))?;
    } else {
        if !args.yes {
            if !is_interactive() {
                bail!("delete requires --yes when terminal is not interactive");
            }
            let typed_name = Text::new(&format!(
                "Type project name to move '{}' to trash:",
                display_path(&project.path)
            ))
            .prompt()
            .context("delete confirmation cancelled")?;
            if typed_name != project.name {
                bail!("typed project name did not match; cancelled");
            }
        }
        trash::delete(&project.path)
            .with_context(|| format!("failed to move {} to trash", project.path.display()))?;
    }

    registry.remove_project(project.id)?;
    println!("Deleted {} ({})", project.name, project.public_id);
    Ok(())
}

fn cmd_doc(registry: &Registry, config: &Config, args: DocArgs) -> Result<()> {
    if [args.raw, args.path, args.edit, args.create]
        .into_iter()
        .filter(|flag| *flag)
        .count()
        > 1
    {
        bail!("choose only one of --raw, --path, --edit, or --create");
    }

    let project = registry.resolve_selector(&args.selector, is_interactive())?;
    ensure_project_available(&project)?;
    let doc_path = resolved_doc_path(&project)?;

    if args.path {
        println!("{}", doc_path.display());
        return Ok(());
    }

    if args.edit || args.create {
        ensure_doc_exists_for_edit(&doc_path, args.create, args.yes)?;
        let invocation = resolve_editor_invocation(config, &project, None, &doc_path)?;
        let mut child = spawn_editor_process(&invocation)?;
        wait_for_editor(&mut child)?;
        return Ok(());
    }

    let content = fs::read_to_string(&doc_path)
        .with_context(|| format!("failed to read doc {}", doc_path.display()))?;
    if args.raw || !is_markdown_path(&doc_path) {
        print!("{content}");
    } else {
        print!("{}", render_markdown_to_terminal_text(&content));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EditorInvocation {
    Direct { program: String, target: PathBuf },
    Shell { command: String },
}

fn resolve_editor_invocation(
    config: &Config,
    project: &Project,
    override_value: Option<String>,
    target: &Path,
) -> Result<EditorInvocation> {
    if let Some(value) = override_value {
        return editor_value_to_invocation(&value, target);
    }
    if let Some(command) = project
        .editor_command
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        return Ok(EditorInvocation::Shell {
            command: render_command_template(command, target),
        });
    }
    if !config.editor_command.is_empty() {
        return Ok(EditorInvocation::Shell {
            command: render_command_template(&config.editor_command, target),
        });
    }
    if let Some(editor) = project.editor.as_deref().filter(|value| !value.is_empty()) {
        validate_editor_name(editor)?;
        return Ok(EditorInvocation::Direct {
            program: editor.to_string(),
            target: target.to_path_buf(),
        });
    }
    validate_editor_name(&config.editor)?;
    Ok(EditorInvocation::Direct {
        program: config.editor.clone(),
        target: target.to_path_buf(),
    })
}

fn editor_value_to_invocation(value: &str, target: &Path) -> Result<EditorInvocation> {
    if value.contains("{path}") || value.contains(char::is_whitespace) {
        Ok(EditorInvocation::Shell {
            command: render_command_template(value, target),
        })
    } else {
        validate_editor_name(value)?;
        Ok(EditorInvocation::Direct {
            program: value.to_string(),
            target: target.to_path_buf(),
        })
    }
}

fn render_command_template(template: &str, target: &Path) -> String {
    template.replace("{path}", &shell_quote_path(target))
}

fn shell_quote_path(path: &Path) -> String {
    let raw = path_to_string(path);
    #[cfg(windows)]
    {
        format!("\"{}\"", raw.replace('"', "\\\""))
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", raw.replace('\'', "'\"'\"'"))
    }
}

fn spawn_editor_process(invocation: &EditorInvocation) -> Result<Child> {
    match invocation {
        EditorInvocation::Direct { program, target } => ProcessCommand::new(program)
            .arg(target)
            .spawn()
            .with_context(|| format!("failed to spawn editor '{program}'")),
        EditorInvocation::Shell { command } => {
            #[cfg(windows)]
            let mut process = {
                let mut process = ProcessCommand::new("cmd");
                process.arg("/C").arg(command);
                process
            };
            #[cfg(not(windows))]
            let mut process = {
                let mut process = ProcessCommand::new("sh");
                process.arg("-c").arg(command);
                process
            };
            process
                .spawn()
                .with_context(|| format!("failed to spawn editor command '{command}'"))
        }
    }
}

fn wait_for_editor(child: &mut Child) -> Result<()> {
    let status = child.wait().context("failed to wait for editor")?;
    if !status.success() {
        bail!("editor exited with status {status}");
    }
    Ok(())
}

fn ensure_project_available(project: &Project) -> Result<()> {
    if project.missing {
        bail!(
            "project path is missing: {}\nUse `cb edit {} --path <new-path>` to relocate it, or `cb remove {} --yes` to remove the registry entry.",
            project.path.display(),
            project.public_id,
            project.public_id
        );
    }
    Ok(())
}

fn resolved_doc_path(project: &Project) -> Result<PathBuf> {
    let doc_path = Path::new(&project.doc_path);
    if doc_path.is_absolute() {
        bail!("stored doc path must be relative: {}", project.doc_path);
    }
    if !is_text_doc_path(doc_path) {
        bail!("doc path must be markdown or plain text");
    }
    Ok(project.path.join(doc_path))
}

fn ensure_doc_exists_for_edit(doc_path: &Path, create_requested: bool, yes: bool) -> Result<()> {
    if doc_path.exists() {
        if create_requested {
            println!("Doc already exists: {}", doc_path.display());
            println!("Opening existing file.");
        }
        return Ok(());
    }

    if !create_requested {
        if !is_interactive() {
            bail!(
                "doc does not exist: {}. Use --create to create it.",
                doc_path.display()
            );
        }
        let confirmed = Confirm::new(&format!(
            "Doc does not exist: {}. Create it?",
            doc_path.display()
        ))
        .with_default(false)
        .prompt()
        .context("doc creation confirmation cancelled")?;
        if !confirmed {
            bail!("doc creation cancelled");
        }
    } else if !yes && !is_interactive() {
        bail!("creating docs in non-interactive mode requires --yes");
    } else if !yes {
        let confirmed = Confirm::new(&format!(
            "Create parent directories and doc file {}?",
            doc_path.display()
        ))
        .with_default(false)
        .prompt()
        .context("doc creation confirmation cancelled")?;
        if !confirmed {
            bail!("doc creation cancelled");
        }
    }

    if let Some(parent) = doc_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(doc_path, "")?;
    Ok(())
}

fn is_markdown_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("md" | "markdown")
    )
}

fn render_markdown_to_terminal_text(markdown: &str) -> String {
    let mut output = String::new();
    let mut in_heading = false;
    let mut in_code_block = false;
    let mut list_depth = 0_usize;

    for event in MarkdownParser::new(markdown) {
        match event {
            MarkdownEvent::Start(Tag::Heading { level, .. }) => {
                trim_trailing_spaces(&mut output);
                if !output.ends_with('\n') && !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&"#".repeat(heading_level_number(level)));
                output.push(' ');
                in_heading = true;
            }
            MarkdownEvent::End(TagEnd::Heading(_)) => {
                in_heading = false;
                output.push_str("\n\n");
            }
            MarkdownEvent::Start(Tag::Paragraph) => {}
            MarkdownEvent::End(TagEnd::Paragraph) => output.push_str("\n\n"),
            MarkdownEvent::Start(Tag::List(_)) => {
                list_depth += 1;
            }
            MarkdownEvent::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                output.push('\n');
            }
            MarkdownEvent::Start(Tag::Item) => {
                output.push_str(&"  ".repeat(list_depth.saturating_sub(1)));
                output.push_str("- ");
            }
            MarkdownEvent::End(TagEnd::Item) => output.push('\n'),
            MarkdownEvent::Start(Tag::BlockQuote(_)) => output.push_str("> "),
            MarkdownEvent::End(TagEnd::BlockQuote(_)) => output.push('\n'),
            MarkdownEvent::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                output.push_str("```\n");
            }
            MarkdownEvent::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("```\n\n");
            }
            MarkdownEvent::Text(text) => output.push_str(&text),
            MarkdownEvent::Code(code) => {
                if in_code_block {
                    output.push_str(&code);
                } else {
                    output.push('`');
                    output.push_str(&code);
                    output.push('`');
                }
            }
            MarkdownEvent::SoftBreak => output.push(if in_heading { ' ' } else { '\n' }),
            MarkdownEvent::HardBreak => output.push('\n'),
            MarkdownEvent::Rule => output.push_str("\n---\n"),
            MarkdownEvent::Html(html) => output.push_str(&html),
            MarkdownEvent::FootnoteReference(reference) => {
                output.push_str("[^");
                output.push_str(&reference);
                output.push(']');
            }
            MarkdownEvent::TaskListMarker(checked) => {
                output.push_str(if checked { "[x] " } else { "[ ] " });
            }
            _ => {}
        }
    }

    trim_trailing_spaces(&mut output);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn heading_level_number(level: pulldown_cmark::HeadingLevel) -> usize {
    match level {
        pulldown_cmark::HeadingLevel::H1 => 1,
        pulldown_cmark::HeadingLevel::H2 => 2,
        pulldown_cmark::HeadingLevel::H3 => 3,
        pulldown_cmark::HeadingLevel::H4 => 4,
        pulldown_cmark::HeadingLevel::H5 => 5,
        pulldown_cmark::HeadingLevel::H6 => 6,
    }
}

fn trim_trailing_spaces(output: &mut String) {
    while output.ends_with(' ') || output.ends_with('\t') {
        output.pop();
    }
}

fn validate_delete_target(path: &Path) -> Result<()> {
    let canonical = canonicalize_existing(path)?;
    let root = Path::new(std::path::MAIN_SEPARATOR_STR);
    if canonical == root {
        bail!("refusing to delete filesystem root");
    }
    if let Some(home) = env::var_os("HOME").map(PathBuf::from)
        && canonical == home
    {
        bail!("refusing to delete home directory");
    }
    if let Ok(current_dir) = env::current_dir().and_then(|path| path.canonicalize())
        && canonical == current_dir
    {
        bail!("refusing to delete current working directory");
    }
    if canonical.components().count() <= 2 {
        bail!(
            "refusing to delete shallow system path: {}",
            canonical.display()
        );
    }
    Ok(())
}

fn cmd_tui(registry: &Registry, config: Config, config_path: PathBuf) -> Result<()> {
    if !is_interactive() {
        bail!("tui requires an interactive terminal");
    }

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_tui(registry, config, config_path, &mut terminal);
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )
    .ok();
    terminal.show_cursor().ok();
    result
}

fn run_tui(
    registry: &Registry,
    mut config: Config,
    config_path: PathBuf,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    let mut app = TuiApp::new(registry, &config)?;
    loop {
        terminal.draw(|frame| render_tui(frame, &mut app, &config))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    if handle_tui_key(key, &mut app, registry, &mut config, &config_path)? {
                        break;
                    }
                }
                CrosstermEvent::Mouse(mouse) => {
                    handle_tui_mouse(mouse, &mut app)?;
                }
                CrosstermEvent::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TuiFocus {
    Left,
    Preview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreviewTab {
    Docs,
    Tree,
}

#[derive(Debug)]
struct TuiApp {
    projects: Vec<Project>,
    filtered: Vec<usize>,
    search: String,
    selected: usize,
    focus: TuiFocus,
    tab: PreviewTab,
    docs_scroll: u16,
    tree_scroll: u16,
    status_message: Option<String>,
    docs_tab_rect: Rect,
    tree_tab_rect: Rect,
}

impl TuiApp {
    fn new(registry: &Registry, config: &Config) -> Result<Self> {
        let mut app = Self {
            projects: Vec::new(),
            filtered: Vec::new(),
            search: String::new(),
            selected: 0,
            focus: TuiFocus::Left,
            tab: PreviewTab::Docs,
            docs_scroll: 0,
            tree_scroll: 0,
            status_message: None,
            docs_tab_rect: Rect::default(),
            tree_tab_rect: Rect::default(),
        };
        app.reload(registry, config)?;
        Ok(app)
    }

    fn reload(&mut self, registry: &Registry, config: &Config) -> Result<()> {
        self.projects = registry.all_projects(ListArgs {
            json: false,
            tag: None,
            missing: false,
            sort: config.tui.sort_mode,
            order: config.tui.sort_order,
        })?;
        self.apply_filter();
        Ok(())
    }

    fn apply_filter(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered = self
            .projects
            .iter()
            .enumerate()
            .filter_map(|(index, project)| {
                if query.is_empty() || tui_project_matches(project, &query) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn selected_project(&self) -> Option<&Project> {
        self.filtered
            .get(self.selected)
            .and_then(|index| self.projects.get(*index))
    }

    fn reset_preview_scroll(&mut self) {
        self.docs_scroll = 0;
        self.tree_scroll = 0;
    }
}

fn tui_project_matches(project: &Project, query: &str) -> bool {
    project.name.to_lowercase().contains(query)
        || path_to_string(&project.path).to_lowercase().contains(query)
        || project.tags.iter().any(|tag| tag.contains(query))
}

fn handle_tui_key(
    key: KeyEvent,
    app: &mut TuiApp,
    registry: &Registry,
    config: &mut Config,
    config_path: &Path,
) -> Result<bool> {
    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('r') {
        config.tui.sort_mode = next_sort_mode(config.tui.sort_mode);
        config.save(config_path)?;
        app.reload(registry, config)?;
        app.status_message = Some(format!("Sort: {:?}", config.tui.sort_mode).to_lowercase());
        return Ok(false);
    }
    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('o') {
        config.tui.sort_order = match config.tui.sort_order {
            SortOrder::Asc => SortOrder::Desc,
            SortOrder::Desc => SortOrder::Asc,
        };
        config.save(config_path)?;
        app.reload(registry, config)?;
        app.status_message = Some(format!("Order: {:?}", config.tui.sort_order).to_lowercase());
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc => {
            if !app.search.is_empty() {
                app.search.clear();
                app.apply_filter();
            } else if app.focus == TuiFocus::Preview {
                app.focus = TuiFocus::Left;
            } else {
                return Ok(true);
            }
        }
        KeyCode::Tab => {
            app.focus = match app.focus {
                TuiFocus::Left => TuiFocus::Preview,
                TuiFocus::Preview => TuiFocus::Left,
            };
        }
        KeyCode::Char('/') => app.focus = TuiFocus::Left,
        KeyCode::Char('1') => app.tab = PreviewTab::Docs,
        KeyCode::Char('2') => app.tab = PreviewTab::Tree,
        KeyCode::Enter => open_selected_from_tui(app, registry, config)?,
        KeyCode::Char('d') => edit_selected_doc_from_tui(app, config)?,
        KeyCode::Char('e') => {
            if let Some(project) = app.selected_project().cloned() {
                cmd_edit(registry, EditArgs::from_project_selector(project.public_id))?;
                app.reload(registry, config)?;
            }
        }
        KeyCode::Up => {
            if app.focus == TuiFocus::Left {
                app.selected = app.selected.saturating_sub(1);
                app.reset_preview_scroll();
            } else {
                decrement_preview_scroll(app);
            }
        }
        KeyCode::Down => {
            if app.focus == TuiFocus::Left {
                if app.selected + 1 < app.filtered.len() {
                    app.selected += 1;
                    app.reset_preview_scroll();
                }
            } else {
                increment_preview_scroll(app, 1);
            }
        }
        KeyCode::PageUp => decrement_preview_scroll_by(app, 10),
        KeyCode::PageDown => increment_preview_scroll(app, 10),
        KeyCode::Backspace => {
            if app.focus == TuiFocus::Left {
                app.search.pop();
                app.apply_filter();
                app.reset_preview_scroll();
            }
        }
        KeyCode::Char(ch)
            if app.focus == TuiFocus::Left && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.search.push(ch);
            app.apply_filter();
            app.reset_preview_scroll();
        }
        _ => {}
    }
    Ok(false)
}

impl EditArgs {
    fn from_project_selector(selector: String) -> Self {
        Self {
            selector,
            name: None,
            doc_path: None,
            editor: None,
            editor_command: None,
            path: None,
            tags: Vec::new(),
            add_tags: Vec::new(),
            remove_tags: Vec::new(),
        }
    }
}

fn open_selected_from_tui(app: &mut TuiApp, registry: &Registry, config: &Config) -> Result<()> {
    let Some(project) = app.selected_project().cloned() else {
        return Ok(());
    };
    match ensure_project_available(&project)
        .and_then(|_| resolve_editor_invocation(config, &project, None, &project.path))
        .and_then(|invocation| {
            let mut child = spawn_editor_process(&invocation)?;
            registry.touch_last_opened(project.id)?;
            wait_for_editor(&mut child)
        }) {
        Ok(()) => app.status_message = Some(format!("Opened {}", project.name)),
        Err(error) => app.status_message = Some(error.to_string()),
    }
    Ok(())
}

fn edit_selected_doc_from_tui(app: &mut TuiApp, config: &Config) -> Result<()> {
    let Some(project) = app.selected_project().cloned() else {
        return Ok(());
    };
    let result = ensure_project_available(&project).and_then(|_| {
        let doc_path = resolved_doc_path(&project)?;
        ensure_doc_exists_for_edit(&doc_path, true, true)?;
        let invocation = resolve_editor_invocation(config, &project, None, &doc_path)?;
        let mut child = spawn_editor_process(&invocation)?;
        wait_for_editor(&mut child)
    });
    match result {
        Ok(()) => {
            app.docs_scroll = 0;
            app.status_message = Some(format!("Edited docs for {}", project.name));
        }
        Err(error) => app.status_message = Some(error.to_string()),
    }
    Ok(())
}

fn handle_tui_mouse(mouse: MouseEvent, app: &mut TuiApp) -> Result<()> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let position = Position::new(mouse.column, mouse.row);
            if app.docs_tab_rect.contains(position) {
                app.tab = PreviewTab::Docs;
                app.focus = TuiFocus::Preview;
            } else if app.tree_tab_rect.contains(position) {
                app.tab = PreviewTab::Tree;
                app.focus = TuiFocus::Preview;
            }
        }
        MouseEventKind::ScrollDown => increment_preview_scroll(app, 3),
        MouseEventKind::ScrollUp => decrement_preview_scroll_by(app, 3),
        _ => {}
    }
    Ok(())
}

fn increment_preview_scroll(app: &mut TuiApp, amount: u16) {
    match app.tab {
        PreviewTab::Docs => app.docs_scroll = app.docs_scroll.saturating_add(amount),
        PreviewTab::Tree => app.tree_scroll = app.tree_scroll.saturating_add(amount),
    }
}

fn decrement_preview_scroll(app: &mut TuiApp) {
    decrement_preview_scroll_by(app, 1);
}

fn decrement_preview_scroll_by(app: &mut TuiApp, amount: u16) {
    match app.tab {
        PreviewTab::Docs => app.docs_scroll = app.docs_scroll.saturating_sub(amount),
        PreviewTab::Tree => app.tree_scroll = app.tree_scroll.saturating_sub(amount),
    }
}

fn next_sort_mode(mode: SortMode) -> SortMode {
    match mode {
        SortMode::Recent => SortMode::Name,
        SortMode::Name => SortMode::Path,
        SortMode::Path => SortMode::Created,
        SortMode::Created => SortMode::Recent,
    }
}

fn render_tui(frame: &mut Frame<'_>, app: &mut TuiApp, config: &Config) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(outer[0]);
    render_project_pane(frame, app, panes[0]);
    render_preview_pane(frame, app, config, panes[1]);
    render_status_bar(frame, app, config, outer[1]);
}

fn render_project_pane(frame: &mut Frame<'_>, app: &mut TuiApp, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    let search_style = if app.focus == TuiFocus::Left {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(app.search.as_str())
            .block(Block::default().title("Search").borders(Borders::ALL))
            .style(search_style),
        chunks[0],
    );

    let items = app
        .filtered
        .iter()
        .filter_map(|index| app.projects.get(*index))
        .map(|project| {
            let status = if project.missing { "missing" } else { "ok" };
            let git = if project.git_root.is_some() {
                " git"
            } else {
                ""
            };
            let tags = if project.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", project.tags.join(","))
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(&project.name, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{tags}{git}")),
                ]),
                Line::from(Span::styled(
                    format!("{}  {status}", display_path(&project.path)),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    if !app.filtered.is_empty() {
        state.select(Some(app.selected));
    }
    let block_title = format!("Projects ({}/{})", app.filtered.len(), app.projects.len());
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title(block_title).borders(Borders::ALL))
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
            .highlight_symbol("> "),
        chunks[1],
        &mut state,
    );
}

fn render_preview_pane(frame: &mut Frame<'_>, app: &mut TuiApp, config: &Config, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    let tab_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(1),
        ])
        .split(chunks[0]);
    app.docs_tab_rect = tab_chunks[0];
    app.tree_tab_rect = tab_chunks[1];
    let selected = match app.tab {
        PreviewTab::Docs => 0,
        PreviewTab::Tree => 1,
    };
    frame.render_widget(
        Tabs::new(["Docs", "Tree"])
            .select(selected)
            .block(Block::default().title("Preview").borders(Borders::ALL))
            .style(Style::default())
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        chunks[0],
    );

    let (title, lines, scroll) = match app.tab {
        PreviewTab::Docs => (
            "Docs",
            app.selected_project()
                .map(load_docs_preview)
                .unwrap_or_else(|| vec!["No project selected".to_string()]),
            app.docs_scroll,
        ),
        PreviewTab::Tree => (
            "Tree",
            app.selected_project()
                .map(|project| load_tree_preview(project, &config.tree))
                .unwrap_or_else(|| vec!["No project selected".to_string()]),
            app.tree_scroll,
        ),
    };
    let text = lines.join("\n");
    let focus_style = if app.focus == TuiFocus::Preview {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().title(title).borders(Borders::ALL))
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false })
            .style(focus_style),
        chunks[1],
    );
}

fn render_status_bar(frame: &mut Frame<'_>, app: &TuiApp, config: &Config, area: Rect) {
    let message = app.status_message.as_deref().unwrap_or("");
    let status = format!(
        " Sort: {:?} {:?} | {} projects | Focus: {:?} | Ctrl+r sort | Ctrl+o order | Enter open | d docs | q quit {}",
        config.tui.sort_mode,
        config.tui.sort_order,
        app.filtered.len(),
        app.focus,
        message
    );
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::Black).bg(Color::White)),
        area,
    );
}

fn load_docs_preview(project: &Project) -> Vec<String> {
    if project.missing {
        return vec![
            "Project path is missing.".to_string(),
            format!("Path: {}", project.path.display()),
            "Use edit/relocate from CLI or remove the entry.".to_string(),
        ];
    }
    let doc_path = match resolved_doc_path(project) {
        Ok(path) => path,
        Err(error) => return vec![error.to_string()],
    };
    if !doc_path.exists() {
        return vec![
            "Documentation file is missing.".to_string(),
            format!("Expected: {}", doc_path.display()),
            "Press d to create/open it.".to_string(),
        ];
    }
    match fs::read_to_string(&doc_path) {
        Ok(content) if is_markdown_path(&doc_path) => render_markdown_to_terminal_text(&content)
            .lines()
            .map(ToOwned::to_owned)
            .collect(),
        Ok(content) => content.lines().map(ToOwned::to_owned).collect(),
        Err(error) => vec![format!("Failed to read docs: {error}")],
    }
}

fn load_tree_preview(project: &Project, config: &TreeConfig) -> Vec<String> {
    if project.missing {
        return vec![
            "Project path is missing.".to_string(),
            format!("Path: {}", project.path.display()),
        ];
    }
    match generate_tree_lines(&project.path, config) {
        Ok(lines) => lines,
        Err(error) => vec![format!("Failed to load tree: {error}")],
    }
}

fn generate_tree_lines(root: &Path, config: &TreeConfig) -> Result<Vec<String>> {
    let mut builder = WalkBuilder::new(root);
    builder
        .max_depth(Some(config.max_depth.saturating_add(1)))
        .hidden(false)
        .git_ignore(config.respect_gitignore)
        .git_global(config.respect_gitignore)
        .git_exclude(config.respect_gitignore)
        .sort_by_file_path(|left, right| left.cmp(right));
    let mut lines = Vec::new();
    let mut entries_seen = 0_usize;
    lines.push(format!(
        "{}/",
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(".")
    ));
    for entry in builder.build().skip(1) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                lines.push(format!("! {error}"));
                continue;
            }
        };
        let path = entry.path();
        if should_skip_tree_path(path, root, config) {
            continue;
        }
        entries_seen += 1;
        if entries_seen > config.max_entries {
            lines.push("... tree truncated".to_string());
            break;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let depth = relative.components().count().saturating_sub(1);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");
        let suffix = if path.is_dir() { "/" } else { "" };
        lines.push(format!("{}{}{}", "  ".repeat(depth), name, suffix));
    }
    if lines.len() == 1 {
        lines.push("(empty)".to_string());
    }
    Ok(lines)
}

fn should_skip_tree_path(path: &Path, root: &Path, config: &TreeConfig) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | ".venv" | "__pycache__"
    ) {
        return true;
    }
    if !config.show_hidden && name.starts_with('.') && name != ".env.example" {
        return true;
    }
    relative.components().count() > config.max_depth
}

fn cmd_config(path: PathBuf, mut config: Config, args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::Get { key } => {
            if let Some(key) = key {
                println!("{}", get_config_value(&config, &key)?);
            } else {
                println!("{}", toml::to_string_pretty(&config)?);
            }
        }
        ConfigCommand::Set { key, value } => {
            set_config_value(&mut config, &key, value)?;
            config.save(&path)?;
            println!("Updated config: {}", path.display());
        }
    }
    Ok(())
}

fn get_config_value(config: &Config, key: &str) -> Result<String> {
    match key {
        "editor" => Ok(config.editor.clone()),
        "editor_command" => Ok(config.editor_command.clone()),
        "tree.max_depth" => Ok(config.tree.max_depth.to_string()),
        "tree.max_entries" => Ok(config.tree.max_entries.to_string()),
        "tree.show_hidden" => Ok(config.tree.show_hidden.to_string()),
        "tree.respect_gitignore" => Ok(config.tree.respect_gitignore.to_string()),
        "tui.sort_mode" => Ok(format!("{:?}", config.tui.sort_mode).to_lowercase()),
        "tui.sort_order" => Ok(format!("{:?}", config.tui.sort_order).to_lowercase()),
        _ => bail!("unknown config key: {key}"),
    }
}

fn set_config_value(config: &mut Config, key: &str, value: String) -> Result<()> {
    match key {
        "editor" => {
            validate_editor_name(&value)?;
            config.editor = value;
        }
        "editor_command" => {
            validate_editor_command(&value, is_interactive())?;
            config.editor_command = value;
        }
        "tree.max_depth" => config.tree.max_depth = value.parse()?,
        "tree.max_entries" => config.tree.max_entries = value.parse()?,
        "tree.show_hidden" => config.tree.show_hidden = value.parse()?,
        "tree.respect_gitignore" => config.tree.respect_gitignore = value.parse()?,
        "tui.sort_mode" => {
            config.tui.sort_mode = parse_sort_mode(&value)?;
        }
        "tui.sort_order" => {
            config.tui.sort_order = parse_sort_order(&value)?;
        }
        _ => bail!("unknown config key: {key}"),
    }
    Ok(())
}

fn print_projects(projects: Vec<Project>, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&projects)?);
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_header(vec!["ID", "Name", "Path", "Tags", "Git", "Status"]);
    for project in projects {
        table.add_row(vec![
            Cell::new(project.public_id),
            Cell::new(project.name),
            Cell::new(display_path(&project.path)),
            Cell::new(project.tags.join(",")),
            Cell::new(if project.git_root.is_some() {
                "yes"
            } else {
                "no"
            }),
            Cell::new(if project.missing {
                "missing"
            } else {
                "available"
            }),
        ]);
    }
    println!("{table}");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn registry() -> Registry {
        Registry::open_memory().expect("registry")
    }

    fn temp_project() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("project");
        fs::create_dir(&path).expect("project dir");
        (dir, path.canonicalize().expect("canonical path"))
    }

    fn insert_named(registry: &Registry, name: &str, path: PathBuf, tags: &[&str]) -> Project {
        registry
            .insert_project(NewProject {
                name: name.to_string(),
                path,
                git_root: None,
                doc_path: DEFAULT_DOC_PATH.to_string(),
                tags: tags.iter().map(|tag| tag.to_string()).collect(),
            })
            .expect("insert project")
    }

    fn default_config_with_editor(editor: &str) -> Config {
        Config {
            editor: editor.to_string(),
            ..Config::default()
        }
    }

    #[test]
    fn normalizes_and_validates_tags() {
        assert_eq!(normalize_tag("Work").unwrap(), "work");
        assert_eq!(normalize_tag("client-A").unwrap(), "client-a");
        assert!(normalize_tag("bad tag").is_err());
        assert!(normalize_tag("-bad").is_err());
    }

    #[test]
    fn public_ids_have_expected_shape() {
        let registry = registry();
        let id = registry.generate_public_id().unwrap();
        assert!(id.starts_with("cb_"));
        assert_eq!(id.len(), 19);
        assert!(id[3..].chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn prevents_duplicate_paths() {
        let registry = registry();
        let (_dir, path) = temp_project();
        insert_named(&registry, "One", path.clone(), &[]);
        let error = registry
            .insert_project(NewProject {
                name: "Two".to_string(),
                path,
                git_root: None,
                doc_path: DEFAULT_DOC_PATH.to_string(),
                tags: Vec::new(),
            })
            .unwrap_err();
        assert!(error.to_string().contains("already registered"));
    }

    #[test]
    fn searches_name_path_and_tags_case_insensitively() {
        let registry = registry();
        let (_dir, path) = temp_project();
        insert_named(&registry, "API Server", path.clone(), &["Backend"]);

        assert_eq!(registry.search("api").unwrap()[0].name, "API Server");
        assert_eq!(registry.search("backend").unwrap()[0].name, "API Server");
        let path_fragment = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            registry.search(path_fragment).unwrap()[0].name,
            "API Server"
        );
    }

    #[test]
    fn edit_replaces_adds_and_removes_tags() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path, &["work"]);

        let updated = registry
            .update_project(
                project.id,
                ProjectChanges {
                    name: None,
                    path: None,
                    doc_path: None,
                    editor: None,
                    editor_command: None,
                    replace_tags: Some(vec!["backend".to_string()]),
                    add_tags: vec!["rust".to_string()],
                    remove_tags: vec!["backend".to_string()],
                },
            )
            .unwrap();
        assert_eq!(updated.tags, vec!["rust"]);
    }

    #[test]
    fn remove_deletes_registry_entry_only() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path.clone(), &[]);
        registry.remove_project(project.id).unwrap();

        assert!(path.exists());
        assert!(registry.project_by_id(project.id).unwrap().is_none());
    }

    #[test]
    fn config_round_trips_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config::default();
        config.save(&path).unwrap();
        let loaded = Config::load_or_default(&path).unwrap();

        assert_eq!(loaded.editor, "nvim");
        assert_eq!(loaded.tree.max_depth, 4);
        assert_eq!(loaded.tui.sort_mode, SortMode::Recent);
    }

    #[test]
    fn list_sort_keeps_missing_last() {
        let registry = registry();
        let (dir, path) = temp_project();
        insert_named(&registry, "API", path.clone(), &[]);
        drop(dir);

        let (_dir2, path2) = temp_project();
        insert_named(&registry, "Docs", path2, &[]);

        let mut projects = registry
            .all_projects(ListArgs {
                json: false,
                tag: None,
                missing: false,
                sort: SortMode::Name,
                order: SortOrder::Asc,
            })
            .unwrap();
        sort_projects(&mut projects, SortMode::Name, SortOrder::Asc);
        assert_eq!(projects[0].name, "Docs");
        assert!(projects[1].missing);
    }

    #[test]
    fn dedup_tags_sorts_and_deduplicates() {
        let tags = dedup_tags(&["Work".into(), "backend".into(), "work".into()]).unwrap();
        assert_eq!(tags, vec!["backend", "work"]);
    }

    #[test]
    fn resolves_direct_editor_from_config() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path.clone(), &[]);
        let config = default_config_with_editor("true");

        let invocation = resolve_editor_invocation(&config, &project, None, &path).unwrap();

        assert_eq!(
            invocation,
            EditorInvocation::Direct {
                program: "true".to_string(),
                target: path
            }
        );
    }

    #[test]
    fn resolves_override_template_as_shell_command() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path.clone(), &[]);
        let config = Config::default();

        let invocation =
            resolve_editor_invocation(&config, &project, Some("echo {path}".into()), &path)
                .unwrap();

        assert_eq!(
            invocation,
            EditorInvocation::Shell {
                command: format!("echo {}", shell_quote_path(&path))
            }
        );
    }

    #[test]
    fn shell_quotes_paths_with_spaces_and_single_quotes() {
        let path = PathBuf::from("/tmp/a path/it's-here");
        #[cfg(not(windows))]
        assert_eq!(shell_quote_path(&path), "'/tmp/a path/it'\"'\"'s-here'");
    }

    #[test]
    fn touch_last_opened_updates_project() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path, &[]);

        registry.touch_last_opened(project.id).unwrap();
        let updated = registry.project_by_id(project.id).unwrap().unwrap();

        assert!(updated.last_opened_at.is_some());
    }

    #[test]
    fn resolves_relative_doc_path_under_project_root() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path.clone(), &[]);

        assert_eq!(
            resolved_doc_path(&project).unwrap(),
            path.join(DEFAULT_DOC_PATH)
        );
    }

    #[test]
    fn renders_basic_markdown_to_terminal_text() {
        let rendered = render_markdown_to_terminal_text("# Title\n\n- one\n- two\n\n`code`\n");

        assert!(rendered.contains("# Title"));
        assert!(rendered.contains("- one"));
        assert!(rendered.contains("`code`"));
    }

    #[test]
    fn validates_dangerous_delete_targets() {
        assert!(validate_delete_target(Path::new(std::path::MAIN_SEPARATOR_STR)).is_err());
        if let Some(home) = env::var_os("HOME") {
            assert!(validate_delete_target(&PathBuf::from(home)).is_err());
        }
    }

    #[test]
    fn creates_missing_doc_with_parents() {
        let (_dir, path) = temp_project();
        let doc = path.join("docs/overview.md");

        ensure_doc_exists_for_edit(&doc, true, true).unwrap();

        assert!(doc.exists());
        assert_eq!(fs::read_to_string(doc).unwrap(), "");
    }

    #[test]
    fn tree_generation_skips_heavy_and_hidden_paths() {
        let (_dir, path) = temp_project();
        fs::write(path.join("main.rs"), "").unwrap();
        fs::create_dir(path.join(".git")).unwrap();
        fs::write(path.join(".secret"), "").unwrap();
        fs::write(path.join(".env.example"), "").unwrap();
        fs::create_dir(path.join("node_modules")).unwrap();
        fs::write(path.join("node_modules/pkg.js"), "").unwrap();

        let lines = generate_tree_lines(&path, &TreeConfig::default()).unwrap();
        let joined = lines.join("\n");

        assert!(joined.contains("main.rs"));
        assert!(joined.contains(".env.example"));
        assert!(!joined.contains(".secret"));
        assert!(!joined.contains(".git"));
        assert!(!joined.contains("node_modules"));
    }

    #[test]
    fn tree_generation_respects_depth_and_entry_limits() {
        let (_dir, path) = temp_project();
        fs::create_dir(path.join("a")).unwrap();
        fs::create_dir(path.join("a/b")).unwrap();
        fs::write(path.join("a/b/deep.txt"), "").unwrap();
        fs::write(path.join("one.txt"), "").unwrap();
        fs::write(path.join("two.txt"), "").unwrap();

        let config = TreeConfig {
            max_depth: 1,
            max_entries: 2,
            ..TreeConfig::default()
        };
        let lines = generate_tree_lines(&path, &config).unwrap();
        let joined = lines.join("\n");

        assert!(joined.contains("a/"));
        assert!(!joined.contains("deep.txt"));
        assert!(joined.contains("truncated"));
    }

    #[test]
    fn tui_filter_matches_name_path_and_tags() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API Server", path, &["backend"]);

        assert!(tui_project_matches(&project, "api"));
        assert!(tui_project_matches(&project, "backend"));
        assert!(tui_project_matches(&project, "project"));
        assert!(!tui_project_matches(&project, "frontend"));
    }

    #[test]
    fn sort_mode_cycles_in_expected_order() {
        assert_eq!(next_sort_mode(SortMode::Recent), SortMode::Name);
        assert_eq!(next_sort_mode(SortMode::Name), SortMode::Path);
        assert_eq!(next_sort_mode(SortMode::Path), SortMode::Created);
        assert_eq!(next_sort_mode(SortMode::Created), SortMode::Recent);
    }

    #[test]
    fn detects_cb_tui_binary_names() {
        assert!(is_cb_tui_binary_name("cb-tui"));
        assert!(is_cb_tui_binary_name("cb-tui.exe"));
        assert!(!is_cb_tui_binary_name("cb"));
    }

    #[test]
    fn docs_preview_reports_missing_doc() {
        let registry = registry();
        let (_dir, path) = temp_project();
        let project = insert_named(&registry, "API", path, &[]);

        let lines = load_docs_preview(&project);

        assert!(lines.join("\n").contains("Documentation file is missing"));
    }

    #[test]
    fn docs_preview_reports_missing_project() {
        let registry = registry();
        let (dir, path) = temp_project();
        let project = insert_named(&registry, "API", path, &[]);
        drop(dir);
        let project = registry.project_by_id(project.id).unwrap().unwrap();

        let lines = load_docs_preview(&project);

        assert!(lines.join("\n").contains("Project path is missing"));
    }
}
