use super::*;

pub(super) struct AppPaths {
    pub(super) data_dir: PathBuf,
    pub(super) config_file: PathBuf,
    pub(super) db_file: PathBuf,
}

impl AppPaths {
    pub(super) fn new() -> Result<Self> {
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
pub(super) struct Config {
    pub(super) editor: String,
    #[serde(default)]
    pub(super) editor_command: String,
    #[serde(default)]
    pub(super) tree: TreeConfig,
    #[serde(default)]
    pub(super) tui: TuiConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TreeConfig {
    pub(super) max_depth: usize,
    pub(super) max_entries: usize,
    pub(super) show_hidden: bool,
    pub(super) respect_gitignore: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TuiConfig {
    pub(super) sort_mode: SortMode,
    pub(super) sort_order: SortOrder,
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
    pub(super) fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse config {}", path.display()))
    }

    pub(super) fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Project {
    pub(super) id: i64,
    pub(super) public_id: String,
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) git_root: Option<PathBuf>,
    pub(super) doc_path: String,
    pub(super) editor: Option<String>,
    pub(super) editor_command: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) last_opened_at: Option<String>,
    pub(super) tags: Vec<String>,
    pub(super) missing: bool,
}

#[derive(Debug)]
pub(super) struct NewProject {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) git_root: Option<PathBuf>,
    pub(super) doc_path: String,
    pub(super) tags: Vec<String>,
}

#[derive(Debug)]
pub(super) struct ProjectChanges {
    pub(super) name: Option<String>,
    pub(super) path: Option<PathBuf>,
    pub(super) doc_path: Option<String>,
    pub(super) editor: Option<Option<String>>,
    pub(super) editor_command: Option<Option<String>>,
    pub(super) replace_tags: Option<Vec<String>>,
    pub(super) add_tags: Vec<String>,
    pub(super) remove_tags: Vec<String>,
}

pub(super) struct Registry {
    conn: Connection,
}

impl Registry {
    pub(super) fn open(path: &Path) -> Result<Self> {
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
    pub(super) fn open_memory() -> Result<Self> {
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

    pub(super) fn insert_project(&self, project: NewProject) -> Result<Project> {
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

    pub(super) fn generate_public_id(&self) -> Result<String> {
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

    pub(super) fn project_by_id(&self, id: i64) -> Result<Option<Project>> {
        let mut projects = self.projects_where("p.id = ?1", params![id])?;
        Ok(projects.pop())
    }

    pub(super) fn project_by_path(&self, path: &Path) -> Result<Option<Project>> {
        let mut projects = self.projects_where("p.path = ?1", params![path_to_string(path)])?;
        Ok(projects.pop())
    }

    pub(super) fn all_projects(&self, args: ListArgs) -> Result<Vec<Project>> {
        let mut projects = if let Some(tag) = args.tag {
            let tag = normalize_tag(&tag)?;
            self.projects_with_tag(&tag)?
        } else {
            self.projects_where("1 = 1", [])?
        };
        sort_projects(&mut projects, args.sort, args.order);
        Ok(projects)
    }

    pub(super) fn search(&self, query: &str) -> Result<Vec<Project>> {
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

    pub(super) fn resolve_selector(&self, selector: &str, allow_picker: bool) -> Result<Project> {
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

    pub(super) fn update_project(&self, id: i64, changes: ProjectChanges) -> Result<Project> {
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

    pub(super) fn remove_project(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    pub(super) fn touch_last_opened(&self, id: i64) -> Result<()> {
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
