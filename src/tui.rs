use super::*;
use crate::storage::{Config, Project, Registry, TreeConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum EditorInvocation {
    Direct { program: String, target: PathBuf },
    Shell { command: String },
}

pub(super) fn resolve_editor_invocation(
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

pub(super) fn shell_quote_path(path: &Path) -> String {
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

pub(super) fn spawn_editor_process(invocation: &EditorInvocation) -> Result<Child> {
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

pub(super) fn wait_for_editor(child: &mut Child) -> Result<()> {
    let status = child.wait().context("failed to wait for editor")?;
    if !status.success() {
        bail!("editor exited with status {status}");
    }
    Ok(())
}

pub(super) fn ensure_project_available(project: &Project) -> Result<()> {
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

pub(super) fn resolved_doc_path(project: &Project) -> Result<PathBuf> {
    let doc_path = Path::new(&project.doc_path);
    if doc_path.is_absolute() {
        bail!("stored doc path must be relative: {}", project.doc_path);
    }
    if !is_text_doc_path(doc_path) {
        bail!("doc path must be markdown or plain text");
    }
    Ok(project.path.join(doc_path))
}

pub(super) fn ensure_doc_exists_for_edit(
    doc_path: &Path,
    create_requested: bool,
    yes: bool,
) -> Result<()> {
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

pub(super) fn is_markdown_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("md" | "markdown")
    )
}

pub(super) fn render_markdown_to_terminal_text(markdown: &str) -> String {
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

pub(super) fn validate_delete_target(path: &Path) -> Result<()> {
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

pub(super) fn cmd_tui(registry: &Registry, config: Config, config_path: PathBuf) -> Result<()> {
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

pub(super) fn tui_project_matches(project: &Project, query: &str) -> bool {
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

pub(super) fn next_sort_mode(mode: SortMode) -> SortMode {
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

pub(super) fn load_docs_preview(project: &Project) -> Vec<String> {
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

pub(super) fn generate_tree_lines(root: &Path, config: &TreeConfig) -> Result<Vec<String>> {
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
