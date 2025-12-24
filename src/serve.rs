use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use comrak::{markdown_to_html, Options};
use regex::Regex;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

const STYLES_CSS: &str = include_str!("../assets/styles.css");
const MAIN_CSS: &str = include_str!("../assets/main.css");
const BLOG_JS: &str = include_str!("../assets/blog.js");
const PARA_JS: &str = include_str!("../assets/main.js");
const UBUNTU_MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/UbuntuMono-Regular.ttf");
const UBUNTU_MONO_ITALIC: &[u8] = include_bytes!("../assets/fonts/UbuntuMono-Italic.ttf");
const UBUNTU_MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/UbuntuMono-Bold.ttf");
const UBUNTU_MONO_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/UbuntuMono-BoldItalic.ttf");

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
}

struct AppState {
    notes_dir: PathBuf,
}

pub async fn run_server(notes_dir: PathBuf, port: u16) -> Result<()> {
    let state = Arc::new(AppState { notes_dir });

    let app = Router::new()
        .route("/", get(handle_root))
        .route("/search", get(handle_search))
        .route("/fonts/{*path}", get(handle_fonts))
        .route("/{*path}", get(handle_path))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    println!("Serving notes at http://localhost:{port}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_root(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    serve_path(&state.notes_dir, &state.notes_dir, "").await
}

async fn handle_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Html<String>, StatusCode> {
    let query = params.q.unwrap_or_default();
    let notes_canonical = state
        .notes_dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_tree = render_file_tree(&notes_canonical, &notes_canonical)?;

    if query.is_empty() {
        let content = "<p>Enter a search term above.</p>";
        return Ok(Html(wrap_html("Search", content, &file_tree, &query)));
    }

    let output = Command::new("rg")
        .args([
            "--color",
            "never",
            "--line-number",
            "--max-count",
            "3",
            "-C",
            "1",
            "--type",
            "md",
            &query,
        ])
        .current_dir(&state.notes_dir)
        .output()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.is_empty() {
        let content = format!("<h1>No results for \"{}\"</h1>", html_escape(&query));
        return Ok(Html(wrap_html("Search", &content, &file_tree, &query)));
    }

    let content = render_search_results(&stdout, &query);
    Ok(Html(wrap_html(
        &format!("Search: {}", query),
        &content,
        &file_tree,
        &query,
    )))
}

async fn handle_path(State(state): State<Arc<AppState>>, Path(path): Path<String>) -> Response {
    let full_path = state.notes_dir.join(&path);

    if full_path.is_dir() && !path.ends_with('/') {
        return Redirect::permanent(&format!("/{path}/")).into_response();
    }

    match serve_path(&state.notes_dir, &full_path, "").await {
        Ok(html) => html.into_response(),
        Err(status) => status.into_response(),
    }
}

async fn handle_fonts(Path(path): Path<String>) -> Response {
    let (bytes, content_type) = match path.as_str() {
        "UbuntuMono-Regular.ttf" => (UBUNTU_MONO_REGULAR, "font/ttf"),
        "UbuntuMono-Italic.ttf" => (UBUNTU_MONO_ITALIC, "font/ttf"),
        "UbuntuMono-Bold.ttf" => (UBUNTU_MONO_BOLD, "font/ttf"),
        "UbuntuMono-BoldItalic.ttf" => (UBUNTU_MONO_BOLD_ITALIC, "font/ttf"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn serve_path(
    notes_dir: &PathBuf,
    path: &PathBuf,
    query: &str,
) -> Result<Html<String>, StatusCode> {
    let canonical = path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let notes_canonical = notes_dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !canonical.starts_with(&notes_canonical) {
        return Err(StatusCode::FORBIDDEN);
    }

    let file_tree = render_file_tree(&notes_canonical, &notes_canonical)?;

    if canonical.is_file() {
        if canonical.extension().is_some_and(|ext| ext == "md") {
            let content =
                std::fs::read_to_string(&canonical).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let html = render_markdown(&content);
            let title = canonical
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Note");
            Ok(Html(wrap_html(title, &html, &file_tree, query)))
        } else {
            Err(StatusCode::NOT_FOUND)
        }
    } else if canonical.is_dir() {
        let readme = canonical.join("README.md");
        let index = canonical.join("INDEX.md");

        if readme.exists() {
            let content =
                std::fs::read_to_string(&readme).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let html = render_markdown(&content);
            Ok(Html(wrap_html("Notes", &html, &file_tree, query)))
        } else if index.exists() {
            let content =
                std::fs::read_to_string(&index).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let html = render_markdown(&content);
            Ok(Html(wrap_html("Notes", &html, &file_tree, query)))
        } else {
            let html = render_directory(&canonical, notes_dir)?;
            let dir_name = canonical
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Notes");
            Ok(Html(wrap_html(dir_name, &html, &file_tree, query)))
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_search_results(output: &str, query: &str) -> String {
    let mut html = format!(
        "<h1>Search results for \"{}\"</h1>\n",
        html_escape(query)
    );
    let mut current_file: Option<String> = None;
    let mut lines_buffer: Vec<String> = Vec::new();

    let flush_file = |html: &mut String, file: &Option<String>, lines: &mut Vec<String>| {
        if let Some(f) = file {
            if !lines.is_empty() {
                html.push_str(&format!(
                    "<div class=\"search-result\"><a href=\"/{}\">{}</a><pre>{}</pre></div>\n",
                    f,
                    f,
                    lines.join("\n")
                ));
                lines.clear();
            }
        }
    };

    for line in output.lines() {
        if let Some((file_part, rest)) = line.split_once(':') {
            if let Some((_, content)) = rest.split_once(':') {
                if current_file.as_deref() != Some(file_part) {
                    flush_file(&mut html, &current_file, &mut lines_buffer);
                    current_file = Some(file_part.to_string());
                }
                let escaped = html_escape(content);
                let highlighted = escaped.replace(
                    &html_escape(query),
                    &format!("<mark>{}</mark>", html_escape(query)),
                );
                lines_buffer.push(highlighted);
            }
        } else if line.starts_with("--") {
            if !lines_buffer.is_empty() {
                lines_buffer.push("...".to_string());
            }
        }
    }

    flush_file(&mut html, &current_file, &mut lines_buffer);

    if html.contains("search-result") {
        html
    } else {
        format!("<h1>No results for \"{}\"</h1>", html_escape(query))
    }
}

fn process_wiki_links(content: &str) -> String {
    let re = Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("valid regex");

    re.replace_all(content, |caps: &regex::Captures| {
        let target = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let display = caps.get(2).map(|m| m.as_str()).unwrap_or(target);

        let path = if target.ends_with(".md") {
            format!("/{}", target)
        } else {
            format!("/{}.md", target)
        };

        format!("[{}]({})", display, path)
    })
    .to_string()
}

fn render_markdown(content: &str) -> String {
    let processed = process_wiki_links(content);

    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.render.unsafe_ = true;

    markdown_to_html(&processed, &options)
}

fn render_file_tree(dir: &PathBuf, notes_root: &PathBuf) -> Result<String, StatusCode> {
    fn render_tree_recursive(
        dir: &PathBuf,
        notes_root: &PathBuf,
        depth: usize,
    ) -> Result<String, StatusCode> {
        if depth > 3 {
            return Ok(String::new());
        }

        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter_map(|e| e.ok())
            .collect();

        entries.sort_by_key(|e| e.file_name());

        let mut html = String::from("<ul>\n");

        for entry in entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with('.') || name_str.starts_with('_') {
                continue;
            }

            let file_type = entry
                .file_type()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let entry_path = entry.path();
            let relative_path = entry_path
                .strip_prefix(notes_root)
                .unwrap_or(&entry_path)
                .to_string_lossy();

            if file_type.is_dir() {
                let children = render_tree_recursive(&entry_path, notes_root, depth + 1)?;
                if children.contains("<li>") || depth < 1 {
                    html.push_str(&format!(
                        "<li class=\"dir\"><span class=\"toggle\"></span><a href=\"/{path}/\">{name}</a>{children}</li>\n",
                        path = relative_path,
                        name = name_str,
                        children = children
                    ));
                }
            } else if name_str.ends_with(".md") && depth > 0 {
                html.push_str(&format!(
                    "<li><a href=\"/{path}\">{name}</a></li>\n",
                    path = relative_path,
                    name = name_str
                ));
            }
        }

        html.push_str("</ul>");
        Ok(html)
    }

    let mut html = String::from("<nav class=\"file-tree\"><a href=\"/\">Notes</a>");
    html.push_str(&render_tree_recursive(dir, notes_root, 0)?);
    html.push_str("</nav>");
    Ok(html)
}

fn render_directory(dir: &PathBuf, notes_dir: &PathBuf) -> Result<String, StatusCode> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|e| e.ok())
        .collect();

    entries.sort_by_key(|e| e.file_name());

    let mut html = String::from("<ul class=\"file-listing\">\n");

    if dir != notes_dir {
        html.push_str("  <li><a href=\"..\">..</a></li>\n");
    }

    for entry in entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') || name_str.starts_with('_') {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if file_type.is_dir() {
            html.push_str(&format!(
                "  <li><a href=\"{name}/\">{name}/</a></li>\n",
                name = name_str
            ));
        } else if name_str.ends_with(".md") {
            html.push_str(&format!(
                "  <li><a href=\"{name}\">{name}</a></li>\n",
                name = name_str
            ));
        }
    }

    html.push_str("</ul>");
    Ok(html)
}

fn wrap_html(title: &str, content: &str, file_tree: &str, search_query: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - para</title>
    <style>{styles_css}</style>
    <style>{main_css}</style>
    <style>
        body {{
            display: flex;
            flex-direction: column;
            margin: 0;
            min-height: 100vh;
        }}
        .navbar {{
            display: flex;
            align-items: center;
            padding: 0 1.5rem;
            height: 50px;
            background: var(--background-color);
            border-bottom: 1px solid var(--subtitle-color);
        }}
        .navbar .search-form {{
            display: flex;
            flex: 1;
            max-width: 500px;
            gap: 0.5rem;
        }}
        .navbar .search-form input {{
            flex: 1;
            padding: 0.4rem 0.75rem;
            font-family: inherit;
            font-size: inherit;
            background: var(--code-background);
            color: var(--text-color);
            border: 1px solid var(--subtitle-color);
            outline: none;
        }}
        .navbar .search-form input:focus {{
            border-color: var(--accent-color);
        }}
        .navbar .search-form button {{
            padding: 0.4rem 1rem;
            font-family: inherit;
            font-size: inherit;
            background: var(--subtitle-color);
            color: var(--background-color);
            border: 1px solid var(--subtitle-color);
            cursor: pointer;
        }}
        .navbar .search-form button:hover {{
            background: var(--accent-color);
            border-color: var(--accent-color);
        }}
        .content-wrapper {{
            display: flex;
            flex: 1;
            overflow: hidden;
        }}
        .sidebar {{
            display: flex;
            height: calc(100vh - 50px);
        }}
        .file-tree {{
            width: 280px;
            min-width: 150px;
            max-width: 600px;
            padding: 1rem 1.5rem;
            overflow-y: auto;
            height: 100%;
            box-sizing: border-box;
            background: var(--background-color);
        }}
        .resize-handle {{
            width: 4px;
            cursor: col-resize;
            background: var(--subtitle-color);
            transition: background 0.2s;
        }}
        .resize-handle:hover,
        .resize-handle.dragging {{
            background: var(--accent-color);
        }}
        .file-tree ul {{
            list-style: none;
            padding-left: 1rem;
            margin: 0;
        }}
        .file-tree > ul {{
            padding-left: 0;
        }}
        .file-tree li {{
            padding: 0.15rem 0;
        }}
        .file-tree li.dir {{
            position: relative;
        }}
        .file-tree .toggle {{
            display: inline-block;
            width: 1rem;
            cursor: pointer;
            user-select: none;
        }}
        .file-tree .toggle::before {{
            content: '▼';
            font-size: 0.6rem;
            color: var(--subtitle-color);
            transition: transform 0.15s;
            display: inline-block;
        }}
        .file-tree li.dir.collapsed .toggle::before {{
            transform: rotate(-90deg);
        }}
        .file-tree li.dir.collapsed > ul {{
            display: none;
        }}
        .file-tree a {{
            text-decoration: none;
            color: var(--text-color);
        }}
        .file-tree a:hover {{
            color: var(--accent-color);
        }}
        .file-tree > a {{
            font-weight: bold;
            font-size: 1.1rem;
            display: inline-block;
            margin-bottom: 0.75rem;
            border-bottom: 2px solid var(--accent-color);
            padding-bottom: 2px;
        }}
        main {{
            flex: 1;
            padding: 2rem 2.5rem;
            max-width: 800px;
            overflow-y: auto;
            height: calc(100vh - 50px);
            box-sizing: border-box;
        }}
        main h1:first-child {{
            margin-top: 0;
        }}
        .file-listing {{
            list-style: none;
            padding: 0;
        }}
        .file-listing li {{
            padding: 0.5rem 0;
            border-bottom: 1px solid var(--subtitle-color);
        }}
        .file-listing a {{
            text-decoration: none;
        }}
        .file-listing a:hover {{
            color: var(--accent-color);
        }}
        .search-result {{
            margin-bottom: 1.5rem;
        }}
        .search-result a {{
            color: var(--accent-color);
            text-decoration: none;
            font-weight: bold;
        }}
        .search-result a:hover {{
            text-decoration: underline;
        }}
        .search-result pre {{
            margin-top: 0.5rem;
            white-space: pre-wrap;
            word-break: break-word;
        }}
        mark {{
            background: var(--accent-color);
            color: var(--background-color);
            padding: 0 0.2rem;
        }}
        table {{
            border-collapse: collapse;
            width: 100%;
            margin: 1rem 0;
        }}
        th, td {{
            border: 1px solid var(--subtitle-color);
            padding: 0.5rem 0.75rem;
            text-align: left;
        }}
        th {{
            background: var(--code-background);
        }}
        code {{
            background: var(--code-background);
            padding: 0.1rem 0.3rem;
            border: 1px solid var(--subtitle-color);
        }}
        pre {{
            background: var(--code-background);
            padding: 1rem;
            border: 1px solid var(--subtitle-color);
            overflow-x: auto;
        }}
        pre code {{
            border: none;
            padding: 0;
        }}
    </style>
</head>
<body>
    <nav class="navbar">
        <form class="search-form" action="/search" method="get">
            <input type="text" name="q" placeholder="Search notes..." value="{search_query}" />
            <button type="submit">Search</button>
        </form>
    </nav>
    <div class="content-wrapper">
        <div class="sidebar">
            {file_tree}
            <div class="resize-handle"></div>
        </div>
        <main>
            {content}
        </main>
    </div>
    <script>{blog_js}</script>
    <script>{para_js}</script>
</body>
</html>"#,
        title = title,
        content = content,
        file_tree = file_tree,
        search_query = html_escape(search_query),
        styles_css = STYLES_CSS,
        main_css = MAIN_CSS,
        blog_js = BLOG_JS,
        para_js = PARA_JS
    )
}
