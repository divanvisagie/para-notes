use anyhow::Result;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use comrak::{markdown_to_html, Options};
use futures::{SinkExt, StreamExt};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::broadcast;

const PARA_CSS: &str = include_str!("../assets/para.css");
const PARA_JS: &str = include_str!("../assets/main.js");
const HTMX_JS: &str = include_str!("../assets/htmx.min.js");
const MERMAID_JS: &str = include_str!("../assets/mermaid.min.js");
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
    reload_tx: broadcast::Sender<String>,
}

pub async fn run_server(notes_dir: PathBuf, port: u16) -> Result<()> {
    let (reload_tx, _) = broadcast::channel::<String>(16);

    // Start file watcher
    let watcher_tx = reload_tx.clone();
    let watch_dir = notes_dir.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for watcher");

        rt.block_on(async {
            let (tx, mut rx) = tokio::sync::mpsc::channel(100);

            let mut watcher: RecommendedWatcher =
                notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = tx.blocking_send(event);
                    }
                })
                .expect("Failed to create file watcher");

            watcher
                .watch(&watch_dir, RecursiveMode::Recursive)
                .expect("Failed to watch directory");

            while let Some(event) = rx.recv().await {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let is_md = event.paths.iter().any(|p| {
                        p.extension().is_some_and(|ext| ext == "md")
                    });
                    if is_md {
                        let path = event
                            .paths
                            .first()
                            .and_then(|p| p.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let _ = watcher_tx.send(path);
                    }
                }
            }
        });
    });

    let state = Arc::new(AppState {
        notes_dir,
        reload_tx,
    });

    let app = Router::new()
        .route("/", get(handle_root))
        .route("/search", get(handle_search))
        .route("/ws", get(handle_websocket))
        .route("/fonts/{*path}", get(handle_fonts))
        .route("/{*path}", get(handle_path))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    println!("Serving notes at http://localhost:{port}");
    println!("Live reload enabled - watching for file changes");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_root(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, StatusCode> {
    let is_htmx = headers.contains_key("hx-request");
    serve_path(&state.notes_dir, &state.notes_dir, "", is_htmx).await
}

async fn handle_search(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Response, StatusCode> {
    let is_htmx = headers.contains_key("hx-request");
    let query = params.q.unwrap_or_default();
    let notes_canonical = state
        .notes_dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_tree = render_file_tree(&notes_canonical, &notes_canonical)?;

    if query.is_empty() {
        let content = "<p>Enter a search term above.</p>";
        return Ok(build_response("Search", content, &file_tree, &query, is_htmx));
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
            "-i",
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
        return Ok(build_response("Search", &content, &file_tree, &query, is_htmx));
    }

    let content = render_search_results(&stdout, &query);
    Ok(build_response(
        &format!("Search: {}", query),
        &content,
        &file_tree,
        &query,
        is_htmx,
    ))
}

async fn handle_path(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Response {
    let is_htmx = headers.contains_key("hx-request");
    let full_path = state.notes_dir.join(&path);

    if full_path.is_dir() && !path.ends_with('/') {
        return Redirect::permanent(&format!("/{path}/")).into_response();
    }

    match serve_path(&state.notes_dir, &full_path, "", is_htmx).await {
        Ok(resp) => resp,
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

async fn handle_websocket(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.reload_tx.subscribe()))
}

async fn handle_socket(socket: WebSocket, mut reload_rx: broadcast::Receiver<String>) {
    let (mut sender, mut receiver) = socket.split();

    // Spawn task to forward reload events to the client
    let send_task = tokio::spawn(async move {
        while let Ok(path) = reload_rx.recv().await {
            let msg = serde_json::json!({
                "type": "reload",
                "path": path
            });
            if sender
                .send(Message::Text(msg.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Keep connection alive by consuming incoming messages (pings, etc.)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {
            // Just consume messages to keep connection alive
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}

async fn serve_path(
    notes_dir: &PathBuf,
    path: &PathBuf,
    query: &str,
    is_htmx: bool,
) -> Result<Response, StatusCode> {
    let canonical = path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let notes_canonical = notes_dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !canonical.starts_with(&notes_canonical) {
        return Err(StatusCode::FORBIDDEN);
    }

    let file_tree = render_file_tree(&notes_canonical, &notes_canonical)?;

    if canonical.is_file() {
        let ext = canonical.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "md" {
            let content =
                std::fs::read_to_string(&canonical).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let html = render_markdown(&content);
            let title = canonical
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Note");
            Ok(build_response(title, &html, &file_tree, query, is_htmx))
        } else {
            // Serve static files (images, etc.)
            let content_type = match ext {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                "webp" => "image/webp",
                "pdf" => "application/pdf",
                "css" => "text/css",
                "js" => "application/javascript",
                _ => "application/octet-stream",
            };
            let bytes = std::fs::read(&canonical).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(bytes))
                .unwrap())
        }
    } else if canonical.is_dir() {
        let readme = canonical.join("README.md");
        let index = canonical.join("INDEX.md");

        if readme.exists() {
            let content =
                std::fs::read_to_string(&readme).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let html = render_markdown(&content);
            Ok(build_response("Notes", &html, &file_tree, query, is_htmx))
        } else if index.exists() {
            let content =
                std::fs::read_to_string(&index).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let html = render_markdown(&content);
            Ok(build_response("Notes", &html, &file_tree, query, is_htmx))
        } else {
            let html = render_directory(&canonical, notes_dir)?;
            let dir_name = canonical
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Notes");
            Ok(build_response(dir_name, &html, &file_tree, query, is_htmx))
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

fn build_response(title: &str, content: &str, file_tree: &str, query: &str, is_htmx: bool) -> Response {
    if is_htmx {
        // Return just the main content with a title update
        let html = format!(
            "<title>{title} - para</title>{content}",
            title = title,
            content = content
        );
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(html))
            .unwrap()
    } else {
        Html(wrap_html(title, content, file_tree, query)).into_response()
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
                let escaped_query = regex::escape(&html_escape(query));
                let highlight_re = Regex::new(&format!("(?i){}", escaped_query))
                    .unwrap_or_else(|_| Regex::new("").unwrap());
                let highlighted = highlight_re
                    .replace_all(&escaped, |caps: &regex::Captures| {
                        format!("<mark>{}</mark>", &caps[0])
                    })
                    .to_string();
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
    <style>{para_css}</style>
    <script>{htmx_js}</script>
    <script>{mermaid_js}</script>
    <script>
        (function() {{
            var w = localStorage.getItem('para-sidebar-width');
            if (w) document.documentElement.style.setProperty('--sidebar-width', w + 'px');
        }})();
    </script>
</head>
<body>
    <nav class="navbar">
        <form class="search-form" action="/search" method="get" hx-get="/search" hx-target="main" hx-push-url="true">
            <input type="text" name="q" placeholder="Search notes..." value="{search_query}" />
            <button type="submit">Search</button>
        </form>
    </nav>
    <div class="content-wrapper">
        <div class="sidebar" hx-boost="true" hx-target="main" hx-push-url="true">
            {file_tree}
            <script>
                (function() {{
                    var expanded = JSON.parse(localStorage.getItem('para-expanded-dirs') || '[]');
                    document.querySelectorAll('.file-tree li.dir').forEach(function(li) {{
                        var link = li.querySelector(':scope > a');
                        var path = link ? link.getAttribute('href') : null;
                        if (path && expanded.indexOf(path) !== -1) {{
                            li.classList.add('expanded');
                        }}
                    }});
                }})();
            </script>
            <div class="resize-handle"></div>
        </div>
        <main hx-boost="true" hx-target="main" hx-push-url="true">
            {content}
        </main>
    </div>
    <script>{para_js}</script>
</body>
</html>"#,
        title = title,
        content = content,
        file_tree = file_tree,
        search_query = html_escape(search_query),
        para_css = PARA_CSS,
        htmx_js = HTMX_JS,
        mermaid_js = MERMAID_JS,
        para_js = PARA_JS
    )
}
