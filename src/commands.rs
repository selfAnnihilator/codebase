use super::*;

pub(super) fn cmd_init(registry: &Registry, mut args: InitArgs) -> Result<()> {
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

pub(super) fn cmd_list(registry: &Registry, args: ListArgs) -> Result<()> {
    let json = args.json;
    let projects = registry.all_projects(args)?;
    print_projects(projects, json)
}

pub(super) fn cmd_search(registry: &Registry, args: SearchArgs) -> Result<()> {
    let projects = registry.search(&args.query)?;
    print_projects(projects, args.json)
}

pub(super) fn cmd_open(registry: &Registry, config: &Config, args: OpenArgs) -> Result<()> {
    let project = registry.resolve_selector(&args.selector, is_interactive())?;
    ensure_project_available(&project)?;
    let invocation =
        resolve_editor_invocation(config, &project, args.editor_override, &project.path)?;
    let mut child = spawn_editor_process(&invocation)?;
    registry.touch_last_opened(project.id)?;
    wait_for_editor(&mut child)?;
    Ok(())
}

pub(super) fn cmd_show(registry: &Registry, args: ShowArgs) -> Result<()> {
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

pub(super) fn cmd_edit(registry: &Registry, args: EditArgs) -> Result<()> {
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

pub(super) fn cmd_remove(registry: &Registry, args: RemoveArgs) -> Result<()> {
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

pub(super) fn cmd_delete(registry: &Registry, args: DeleteArgs) -> Result<()> {
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

pub(super) fn cmd_doc(registry: &Registry, config: &Config, args: DocArgs) -> Result<()> {
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

pub(super) fn cmd_config(path: PathBuf, mut config: Config, args: ConfigArgs) -> Result<()> {
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
