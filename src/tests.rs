use super::*;
use crate::storage::TreeConfig;
use crate::tui::{
    EditorInvocation, generate_tree_lines, load_docs_preview, next_sort_mode, shell_quote_path,
    tui_project_matches,
};
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
        resolve_editor_invocation(&config, &project, Some("echo {path}".into()), &path).unwrap();

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
