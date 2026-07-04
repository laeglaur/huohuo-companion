use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, LogicalSize, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

const ARCHIVE_APP_DIR: &str = "/Users/laeglaur/Documents/code/record/archive_app";
const HUUOHUO_DIR: &str = "/Users/laeglaur/Documents/code/record/huohuo";
const ANIME_DIR: &str = "/Users/laeglaur/Documents/code/record/anime";
const NOTEBOOK_APP: &str = "/Users/laeglaur/Documents/code/notebook/src-tauri/target/release/bundle/macos/folia.app";
const NOTEBOOK_APP_DATA_DIR: &str = "/Users/laeglaur/Library/Application Support/com.laeglaur.notebook";
const NOTEBOOK_DATABASE_FILE: &str = "notebook.sqlite3";
const DEFAULT_MODEL_PATH: &str = "/Users/laeglaur/Documents/code/record/huohuo/huohuo.model3.json";
const SETTINGS_FILE: &str = "settings.json";

#[derive(Default)]
struct ArchiveProcess(Mutex<Option<Child>>);

#[derive(Default)]
struct ModelServerProcess(Mutex<HashMap<PathBuf, ModelServerEntry>>);

struct ModelServerEntry {
    port: u16,
    child: Child,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompanionSettings {
    selected_model_path: Option<String>,
    position: Option<WindowPosition>,
    scale: f64,
    #[serde(default)]
    model_scales: HashMap<String, f64>,
    last_archive_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowPosition {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompanionModel {
    id: String,
    display_name: String,
    model_path: String,
    folder: String,
    source_group: String,
    is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveLaunchResult {
    url: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelUrlResult {
    url: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotebookPageSearchResult {
    page_id: String,
    notebook_id: String,
    title: String,
    snippet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotebookCardLaunchResult {
    request_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotebookCardRequest {
    page_id: String,
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve app data dir: {error}"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(SETTINGS_FILE))
}

fn log_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("companion.log"))
}

fn append_log(app: &AppHandle, message: &str) {
    let Ok(path) = log_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

#[tauri::command]
fn log_event(app: AppHandle, message: String) {
    append_log(&app, &message);
}

fn default_settings() -> CompanionSettings {
    CompanionSettings {
        selected_model_path: Some(DEFAULT_MODEL_PATH.to_string()),
        position: None,
        scale: 1.0,
        model_scales: HashMap::new(),
        last_archive_port: None,
    }
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<CompanionSettings, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(default_settings());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read settings: {error}"))?;
    let mut settings: CompanionSettings = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse settings: {error}"))?;
    if settings.scale <= 0.0 {
        settings.scale = 1.0;
    }
    Ok(settings)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: CompanionSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create settings dir: {error}"))?;
    }
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|error| format!("Could not serialize settings: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("Could not write settings: {error}"))
}

#[tauri::command]
fn discover_models() -> Result<Vec<CompanionModel>, String> {
    Ok(discover_models_in_paths(
        Path::new(HUUOHUO_DIR),
        Path::new(ANIME_DIR),
        Path::new(DEFAULT_MODEL_PATH),
    ))
}

#[tauri::command]
fn model_asset_url(
    app: AppHandle,
    state: tauri::State<ModelServerProcess>,
    model_path: String,
) -> Result<ModelUrlResult, String> {
    append_log(&app, &format!("model_asset_url requested: {model_path}"));
    let path = PathBuf::from(model_path);
    if !path.exists() {
        append_log(&app, "model file missing");
        return Err("Model file does not exist.".to_string());
    }
    let model_dir = path
        .parent()
        .ok_or_else(|| "Model file has no parent directory.".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Model file name is not valid UTF-8.".to_string())?;

    let model_dir_key = model_dir.to_path_buf();
    let existing_port = {
        let mut guard = state
            .0
            .lock()
            .map_err(|_| "Model server lock is poisoned.".to_string())?;
        guard.get_mut(&model_dir_key).and_then(|entry| {
            if entry.child.try_wait().ok().flatten().is_none() {
                Some(entry.port)
            } else {
                None
            }
        })
    };

    let port = if let Some(port) = existing_port {
        port
    } else {
        let port = choose_port(9865)?;
        append_log(
            &app,
            &format!("starting model server: dir={} port={port}", model_dir.display()),
        );
        let child = Command::new("python3")
            .arg("-c")
            .arg(cors_static_server_script())
            .arg(port.to_string())
            .current_dir(model_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                append_log(&app, &format!("model server spawn failed: {error}"));
                format!("Could not start model file server: {error}")
            })?;
        {
            let mut guard = state
                .0
                .lock()
                .map_err(|_| "Model server lock is poisoned.".to_string())?;
            guard.insert(model_dir_key, ModelServerEntry { port, child });
        }
        port
    };
    let base = format!("http://127.0.0.1:{port}/");
    wait_for_http(&base, Duration::from_secs(5)).map_err(|error| {
        append_log(&app, &format!("model server did not respond: {error}"));
        error
    })?;
    append_log(&app, &format!("model url ready: {base}{file_name}"));
    Ok(ModelUrlResult {
        url: format!("{base}{file_name}"),
        port,
    })
}

#[tauri::command]
fn open_notebook() -> Result<(), String> {
    let app_path = Path::new(NOTEBOOK_APP);
    if !app_path.exists() {
        return Err(format!("folia.app not found at {NOTEBOOK_APP}"));
    }
    Command::new("open")
        .arg(app_path)
        .spawn()
        .map_err(|error| format!("Could not open folia: {error}"))?;
    Ok(())
}

#[tauri::command]
fn open_icity_login(app: AppHandle) -> Result<(), String> {
    open_icity_window(&app, true).map(|_| ())
}

fn open_icity_window(app: &AppHandle, focus: bool) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("icity") {
        if focus {
            let _ = window.unminimize();
            let _ = window.set_size(LogicalSize::new(390.0, 650.0));
            let _ = window.eval(icity_compact_script());
            let _ = window.set_focus();
        }
        return Ok(window);
    }
    let url = tauri::Url::parse("https://icity.ly/?page=3")
        .map_err(|error| format!("Invalid iCity URL: {error}"))?;
    let window = WebviewWindowBuilder::new(app, "icity", WebviewUrl::External(url))
        .title("iCity")
        .inner_size(390.0, 650.0)
        .min_inner_size(330.0, 500.0)
        .resizable(true)
        .visible(true)
        .initialization_script(icity_compact_script())
        .on_page_load(|window, _| {
            let _ = window.eval(icity_compact_script());
        })
        .build()
        .map_err(|error| format!("Could not create iCity window: {error}"))?;
    if focus {
        let _ = window.eval(icity_compact_script());
        let _ = window.set_focus();
    }
    Ok(window)
}

fn icity_compact_script() -> String {
    r#"
(function() {
  const STYLE_ID = 'huohuo-icity-compact-style';
  const MARK_ATTR = 'data-huohuo-icity-compact';

  function installStyle() {
    let style = document.getElementById(STYLE_ID);
    if (!style) {
      style = document.createElement('style');
      style.id = STYLE_ID;
      document.head.appendChild(style);
    }
    style.textContent = `
      html.huohuo-icity-compact,
      html.huohuo-icity-compact body {
        background: #f7f4ea !important;
        color: #263934 !important;
        font-family: "Avenir Next", "PingFang SC", system-ui, sans-serif !important;
      }

      html.huohuo-icity-compact .post-composer,
      html.huohuo-icity-compact .huohuo-icity-composer {
        width: calc(100% - 28px) !important;
        max-width: 700px !important;
        min-height: 0 !important;
        margin: 12px auto 10px !important;
        padding: 10px 12px !important;
        border: 1px solid rgba(128, 176, 145, .30) !important;
        border-radius: 12px !important;
        background: rgba(255, 253, 247, .94) !important;
        box-shadow: 0 7px 18px rgba(83, 112, 89, .11), inset 0 1px 0 rgba(255,255,255,.92) !important;
      }

      html.huohuo-icity-compact .post-composer textarea,
      html.huohuo-icity-compact .post-composer [contenteditable="true"],
      html.huohuo-icity-compact .post-composer .textarea,
      html.huohuo-icity-compact .post-composer .content,
      html.huohuo-icity-compact .post-composer .body {
        min-height: 92px !important;
        max-height: 150px !important;
        padding: 4px !important;
        color: #263934 !important;
        font-size: 14px !important;
        line-height: 1.55 !important;
      }

      html.huohuo-icity-compact .huohuo-icity-action,
      html.huohuo-icity-compact .huohuo-icity-media {
        min-width: 0 !important;
        height: 30px !important;
        padding: 0 11px !important;
        border-radius: 999px !important;
        font-size: 12px !important;
        line-height: 30px !important;
        box-shadow: none !important;
      }

      html.huohuo-icity-compact .huohuo-icity-action {
        border: 1px solid rgba(49, 127, 121, .28) !important;
        background: #317f79 !important;
        color: #fffdf7 !important;
      }

      html.huohuo-icity-compact .huohuo-icity-media {
        border: 1px solid rgba(128, 176, 145, .26) !important;
        background: rgba(231, 237, 220, .82) !important;
        color: #51706a !important;
      }

      html.huohuo-icity-compact img.avatar,
      html.huohuo-icity-compact .avatar img,
      html.huohuo-icity-compact .post-composer img {
        width: 28px !important;
        height: 28px !important;
        border-radius: 50% !important;
      }

    `;
  }

  function textOf(element) {
    return ((element.value || element.innerText || element.textContent || '') + '').replace(/\s+/g, '');
  }

  function markControls() {
    const hasComposer = Boolean(document.querySelector('.post-composer'));
    document.documentElement.classList.toggle('huohuo-icity-compact', hasComposer);
    document.body?.classList.toggle('huohuo-icity-compact-body', hasComposer);
    if (!hasComposer) return;

    document.querySelectorAll('.post-composer').forEach((element) => {
      element.classList.add('huohuo-icity-composer');
    });

    document.querySelectorAll('button, a, label, input[type="button"], input[type="submit"], .btn, .submit, [role="button"]').forEach((element) => {
      const text = textOf(element);
      if (text === '发布' || text === '发表' || text === '发送') {
        element.classList.add('huohuo-icity-action');
      }
      if (text.includes('添加照片') || text.includes('添加图片') || text.includes('照片') || text.includes('图片')) {
        element.classList.add('huohuo-icity-media');
      }
    });
  }

  function apply() {
    if (!document.head || !document.documentElement) return;
    installStyle();
    markControls();
  }

  apply();
  if (!window[MARK_ATTR]) {
    window[MARK_ATTR] = true;
    document.addEventListener('DOMContentLoaded', apply);
    window.addEventListener('load', apply);
    const observer = new MutationObserver(() => {
      window.clearTimeout(window.__huohuoIcityCompactTimer);
      window.__huohuoIcityCompactTimer = window.setTimeout(apply, 80);
    });
    observer.observe(document.documentElement, { childList: true, subtree: true });
  }
})();
"#.to_string()
}

#[tauri::command]
fn search_notebook_pages(query: String, limit: Option<u32>) -> Result<Vec<NotebookPageSearchResult>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let Some(fts_query) = fts_query_from_search_text(trimmed) else {
        return Ok(Vec::new());
    };
    let db_path = Path::new(NOTEBOOK_APP_DATA_DIR).join(NOTEBOOK_DATABASE_FILE);
    if !db_path.exists() {
        return Err(format!("Folia database not found at {}", db_path.display()));
    }
    let connection = Connection::open(db_path).map_err(|error| error.to_string())?;
    let max_results = i64::from(limit.unwrap_or(12).clamp(1, 30));
    let mut statement = connection
        .prepare(
            "
            SELECT
              fts_pages.page_id,
              pages.notebook_id,
              pages.title,
                snippet(fts_pages, 2, '', '', '...', 12),
                CASE
                  WHEN lower(pages.title) = lower(?3) THEN 0
                  WHEN lower(pages.title) LIKE lower(?3 || '%') THEN 1
                  WHEN lower(pages.title) LIKE lower('%' || ?3 || '%') THEN 2
                  ELSE 3
                END AS title_priority
            FROM fts_pages
            JOIN pages ON pages.id = fts_pages.page_id
            WHERE fts_pages MATCH ?1
            ORDER BY title_priority, rank
            LIMIT ?2
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![fts_query, max_results, trimmed], |row| {
            Ok(NotebookPageSearchResult {
                page_id: row.get(0)?,
                notebook_id: row.get(1)?,
                title: row.get(2)?,
                snippet: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_notebook_card(app: AppHandle, page_id: String) -> Result<NotebookCardLaunchResult, String> {
    let app_path = Path::new(NOTEBOOK_APP);
    if !app_path.exists() {
        return Err(format!("folia.app not found at {NOTEBOOK_APP}"));
    }
    let request_dir = app_data_dir(&app)?.join("folia-card-requests");
    fs::create_dir_all(&request_dir)
        .map_err(|error| format!("Could not create folia request dir: {error}"))?;
    let request_path = request_dir.join(format!("{}.notecard", request_file_stem()));
    let request = NotebookCardRequest { page_id };
    let text = serde_json::to_string(&request)
        .map_err(|error| format!("Could not serialize folia request: {error}"))?;
    fs::write(&request_path, text)
        .map_err(|error| format!("Could not write folia request: {error}"))?;
    Command::new("open")
        .arg("-a")
        .arg(app_path)
        .arg(&request_path)
        .spawn()
        .map_err(|error| format!("Could not open folia card request: {error}"))?;
    Ok(NotebookCardLaunchResult {
        request_path: request_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn launch_archive(app: AppHandle, state: tauri::State<ArchiveProcess>) -> Result<ArchiveLaunchResult, String> {
    launch_archive_from_app(&app, &state)
}

fn launch_archive_from_app(
    app: &AppHandle,
    state: &tauri::State<ArchiveProcess>,
) -> Result<ArchiveLaunchResult, String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "Archive process lock is poisoned.".to_string())?;
    if let Some(child) = guard.as_mut() {
        if child.try_wait().map_err(|error| format!("Could not inspect Archive process: {error}"))?.is_none() {
            let port = load_settings(app.clone())?.last_archive_port.unwrap_or(8765);
            let url = archive_url(port);
            open_archive_window(app, &url)?;
            return Ok(ArchiveLaunchResult { url, port });
        }
        *guard = None;
    }

    let port = choose_port(8765)?;
    let url = archive_url(port);
    let child = Command::new("./archive")
        .arg("open")
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .current_dir(ARCHIVE_APP_DIR)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start Archive service: {error}"))?;
    *guard = Some(child);
    drop(guard);

    wait_for_http(&url, Duration::from_secs(18))?;
    let mut settings = load_settings(app.clone())?;
    settings.last_archive_port = Some(port);
    save_settings(app.clone(), settings)?;
    open_archive_window(app, &url)?;
    Ok(ArchiveLaunchResult { url, port })
}

fn register_global_shortcuts(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let archive_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::ArrowRight);
    append_log(app.handle(), "registering global shortcut option+right");
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_shortcut(archive_shortcut)?
            .with_handler(|app, shortcut, event| {
                append_log(
                    app,
                    &format!(
                        "global shortcut event: option+right matches={} state={:?}",
                        shortcut.matches(Modifiers::ALT, Code::ArrowRight),
                        event.state
                    ),
                );
                if event.state != ShortcutState::Pressed {
                    return;
                }
                if !shortcut.matches(Modifiers::ALT, Code::ArrowRight) {
                    return;
                }
                append_log(app, "global shortcut option+right: launch archive");
                let state = app.state::<ArchiveProcess>();
                if let Err(error) = launch_archive_from_app(app, &state) {
                    append_log(app, &format!("global shortcut launch archive failed: {error}"));
                }
            })
            .build(),
    )?;
    append_log(app.handle(), "registered global shortcut option+right");
    Ok(())
}

#[tauri::command]
fn reset_position(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("companion") {
        window
            .set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: 60, y: 160 }))
            .map_err(|error| format!("Could not reset window position: {error}"))?;
    }
    let mut settings = load_settings(app.clone())?;
    settings.position = Some(WindowPosition { x: 60, y: 160 });
    save_settings(app, settings)
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    stop_archive_process(&app);
    app.exit(0);
}

fn open_archive_window(app: &AppHandle, url: &str) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("archive") {
        window
            .unminimize()
            .map_err(|error| format!("Could not unminimize Archive window: {error}"))?;
        window
            .set_focus()
            .map_err(|error| format!("Could not focus Archive window: {error}"))?;
        return Ok(());
    }
    let parsed = tauri::Url::parse(url).map_err(|error| format!("Invalid Archive URL: {error}"))?;
    WebviewWindowBuilder::new(app, "archive", WebviewUrl::External(parsed))
        .title("Archive")
        .inner_size(1280.0, 860.0)
        .min_inner_size(720.0, 520.0)
        .resizable(true)
        .build()
        .map_err(|error| format!("Could not create Archive window: {error}"))?;
    Ok(())
}

fn archive_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/?companion=external")
}

fn fts_query_from_search_text(query: &str) -> Option<String> {
    let tokens = query
        .split_whitespace()
        .map(|token| token.trim_matches(|character: char| character.is_ascii_punctuation()))
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        let fallback = query.trim();
        if fallback.is_empty() {
            return None;
        }
        return Some(format!("\"{}\"", fallback.replace('"', "\"\"")));
    }
    Some(tokens.join(" "))
}

fn request_file_stem() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("card-{millis}")
}

fn cors_static_server_script() -> &'static str {
    r#"
import http.server
import socketserver
import sys

class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_OPTIONS(self):
        self.send_response(204)
        self.end_headers()

socketserver.ThreadingTCPServer.allow_reuse_address = True
with socketserver.ThreadingTCPServer(("127.0.0.1", int(sys.argv[1])), Handler) as httpd:
    httpd.serve_forever()
"#
}

fn choose_port(preferred: u16) -> Result<u16, String> {
    if is_port_free(preferred) {
        return Ok(preferred);
    }
    for port in 8766..8866 {
        if is_port_free(port) {
            return Ok(port);
        }
    }
    Err("No available local port found for Archive.".to_string())
}

fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn wait_for_http(url: &str, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if Command::new("curl")
            .arg("--noproxy")
            .arg("*")
            .arg("-fsS")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err("Archive service did not respond in time.".to_string())
}

fn discover_models_in_paths(default_dir: &Path, anime_dir: &Path, default_model: &Path) -> Vec<CompanionModel> {
    let mut models = Vec::new();
    collect_model_files(default_dir, "default", default_model, &mut models);
    collect_model_files(anime_dir, "anime", default_model, &mut models);
    models.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
            .then_with(|| a.model_path.cmp(&b.model_path))
    });
    models.dedup_by(|a, b| a.model_path == b.model_path);
    models
}

fn collect_model_files(root: &Path, source_group: &str, default_model: &Path, models: &mut Vec<CompanionModel>) {
    if !root.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_model_files(&path, source_group, default_model, models);
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.ends_with(".model3.json") {
            continue;
        }
        let folder = path.parent().unwrap_or(root).to_string_lossy().to_string();
        let display_name = name.trim_end_matches(".model3.json").replace('_', " ");
        let model_path = path.to_string_lossy().to_string();
        let is_default = path == default_model;
        let id = format!("{source_group}:{}", model_path);
        models.push(CompanionModel {
            id,
            display_name,
            model_path,
            folder,
            source_group: source_group.to_string(),
            is_default,
        });
    }
}

fn stop_archive_process(app: &AppHandle) {
    let state = app.state::<ArchiveProcess>();
    let Ok(mut guard) = state.0.lock() else {
        return;
    };
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn stop_model_server_processes(state: &tauri::State<ModelServerProcess>) {
    let Ok(mut guard) = state.0.lock() else {
        return;
    };
    for (_, mut entry) in guard.drain() {
        let _ = entry.child.kill();
        let _ = entry.child.wait();
    }
}

fn stop_model_server_processes_for_app(app: &AppHandle) {
    let state = app.state::<ModelServerProcess>();
    stop_model_server_processes(&state);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ArchiveProcess::default())
        .manage(ModelServerProcess::default())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            log_event,
            discover_models,
            model_asset_url,
            open_notebook,
            open_icity_login,
            search_notebook_pages,
            open_notebook_card,
            launch_archive,
            reset_position,
            quit_app
        ])
        .setup(|app| {
            register_global_shortcuts(app)?;
            if let Some(window) = app.get_webview_window("companion") {
                let _ = window.set_always_on_top(true);
                let _ = window.set_visible_on_all_workspaces(true);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                stop_archive_process(app);
                stop_model_server_processes_for_app(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_model3_files() {
        let temp = tempfile::tempdir().unwrap();
        let default_dir = temp.path().join("huohuo");
        let anime_dir = temp.path().join("anime");
        fs::create_dir_all(&default_dir).unwrap();
        fs::create_dir_all(anime_dir.join("dog")).unwrap();
        let default_model = default_dir.join("huohuo.model3.json");
        fs::write(&default_model, "{}").unwrap();
        fs::write(anime_dir.join("dog").join("dog.model3.json"), "{}").unwrap();
        fs::write(anime_dir.join("dog").join("dog.vtube.json"), "{}").unwrap();

        let models = discover_models_in_paths(&default_dir, &anime_dir, &default_model);

        assert_eq!(models.len(), 2);
        assert!(models[0].is_default);
        assert_eq!(models[0].display_name, "huohuo");
        assert!(models.iter().any(|model| model.display_name == "dog"));
    }

    #[test]
    fn preferred_port_is_selected_when_free() {
        let port = choose_port(9876).unwrap();
        assert_eq!(port, 9876);
    }
}
