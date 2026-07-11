use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, LogicalSize, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

const NOTEBOOK_DATABASE_FILE: &str = "notebook.sqlite3";
const SETTINGS_FILE: &str = "settings.json";
const FOLIA_EXTERNAL_CARD_REQUESTS_DIR: &str = "external-card-requests";
const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Default)]
struct ArchiveProcess(Mutex<Option<Child>>);

#[derive(Default)]
struct ModelServerProcess(Mutex<HashMap<PathBuf, ModelServerEntry>>);

struct ModelServerEntry {
    port: u16,
    child: Child,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CompanionConfig {
    #[serde(default)]
    live2d_roots: Vec<String>,
    default_model_path: Option<String>,
    archive_app_dir: Option<String>,
    folia_app_path: Option<String>,
    folia_data_dir: Option<String>,
    folia_default_page_id: Option<String>,
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
struct CompanionReaction {
    kind: String,
    name: String,
    group: Option<String>,
    expression: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelBoundsCacheEntry {
    model_path: String,
    bounds_json: String,
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

fn project_root() -> PathBuf {
    Path::new(PROJECT_ROOT)
        .parent()
        .unwrap_or_else(|| Path::new(PROJECT_ROOT))
        .to_path_buf()
}

fn local_dir() -> PathBuf {
    project_root().join("local")
}

fn expand_home_path(value: &str) -> PathBuf {
    if value == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(value)
}

fn resolve_config_path(value: &str) -> PathBuf {
    let path = expand_home_path(value);
    if path.is_absolute() {
        path
    } else {
        project_root().join(path)
    }
}

fn load_companion_config() -> CompanionConfig {
    let path = local_dir().join("config.json");
    let mut config = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<CompanionConfig>(&text).ok())
        .unwrap_or_default();
    if config.live2d_roots.is_empty() {
        config.live2d_roots.push("local/live2d".to_string());
    }
    config
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
        selected_model_path: None,
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
    let text =
        fs::read_to_string(&path).map_err(|error| format!("Could not read settings: {error}"))?;
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
fn is_bounds_calibration_mode() -> bool {
    std::env::var("HUOHUO_BOUNDS_CALIBRATE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[tauri::command]
fn write_precomputed_bounds(bounds_json: String) -> Result<(), String> {
    let output_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "Could not resolve project root.".to_string())?
        .join("src")
        .join("modelBounds.generated.ts");
    let parsed: serde_json::Value = serde_json::from_str(&bounds_json)
        .map_err(|error| format!("Could not parse generated bounds JSON: {error}"))?;
    let pretty = serde_json::to_string_pretty(&parsed)
        .map_err(|error| format!("Could not format generated bounds JSON: {error}"))?;
    let text = format!(
        "import type {{ ModelBoundsSet }} from \"./modelBoundsTypes\";\n\nexport const PRECOMPUTED_MODEL_BOUNDS: Record<string, ModelBoundsSet> = {pretty};\n"
    );
    fs::write(output_path, text).map_err(|error| format!("Could not write bounds file: {error}"))
}

#[tauri::command]
fn load_local_bounds() -> Result<Vec<ModelBoundsCacheEntry>, String> {
    let bounds_dir = local_dir().join("bounds");
    if !bounds_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(bounds_dir).map_err(|error| error.to_string())? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse bounds cache {}: {error}", path.display()))?;
        let Some(model_path) = parsed.get("modelPath").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(bounds) = parsed.get("bounds") else {
            continue;
        };
        entries.push(ModelBoundsCacheEntry {
            model_path: model_path.to_string(),
            bounds_json: serde_json::to_string(bounds).map_err(|error| error.to_string())?,
        });
    }
    Ok(entries)
}

#[tauri::command]
fn write_local_bounds(model_path: String, bounds_json: String) -> Result<(), String> {
    let bounds_dir = local_dir().join("bounds");
    fs::create_dir_all(&bounds_dir)
        .map_err(|error| format!("Could not create bounds dir: {error}"))?;
    let parsed: serde_json::Value = serde_json::from_str(&bounds_json)
        .map_err(|error| format!("Could not parse bounds JSON: {error}"))?;
    let payload = serde_json::json!({
        "modelPath": model_path,
        "bounds": parsed,
    });
    let path = bounds_dir.join(format!("{}.json", stable_file_stem(&model_path)));
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("Could not format bounds JSON: {error}"))?;
    fs::write(path, text).map_err(|error| format!("Could not write bounds cache: {error}"))
}

fn stable_file_stem(value: &str) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

#[tauri::command]
fn discover_models() -> Result<Vec<CompanionModel>, String> {
    let config = load_companion_config();
    Ok(discover_models_from_config(&config))
}

#[tauri::command]
fn model_reactions(model_path: String) -> Result<Vec<CompanionReaction>, String> {
    let path = PathBuf::from(model_path);
    let model_dir = path
        .parent()
        .ok_or_else(|| "Model file has no parent directory.".to_string())?;
    Ok(discover_reactions_in_dir(model_dir))
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
        append_log(
            &app,
            &format!(
                "starting model server: dir={} port=auto",
                model_dir.display()
            ),
        );
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(cors_static_server_script())
            .arg("0")
            .current_dir(model_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                append_log(&app, &format!("model server spawn failed: {error}"));
                format!("Could not start model file server: {error}")
            })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            "Could not read model server port.".to_string()
        })?;
        let mut reader = BufReader::new(stdout);
        let mut port_line = String::new();
        reader.read_line(&mut port_line).map_err(|error| {
            let _ = child.kill();
            format!("Could not read model server port: {error}")
        })?;
        let port = port_line.trim().parse::<u16>().map_err(|error| {
            let _ = child.kill();
            format!("Model server returned invalid port: {error}")
        })?;
        append_log(
            &app,
            &format!(
                "started model server: dir={} port={port}",
                model_dir.display()
            ),
        );
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
    let app_path = find_folia_app().ok_or_else(|| {
        "Folia is not installed or configured. Install Folia or set foliaAppPath in local/config.json."
            .to_string()
    })?;
    Command::new("open")
        .arg(&app_path)
        .spawn()
        .map_err(|error| format!("Could not open folia: {error}"))?;
    Ok(())
}

fn find_folia_app() -> Option<PathBuf> {
    let config = load_companion_config();
    if let Some(path) = config.folia_app_path.as_deref().map(resolve_config_path) {
        if path.exists() {
            return Some(path);
        }
    }
    [
        "/Applications/folia.app",
        "~/Applications/folia.app",
        "/Applications/Folia.app",
        "~/Applications/Folia.app",
    ]
    .iter()
    .map(|path| expand_home_path(path))
    .find(|path| path.exists())
}

fn find_folia_data_dir() -> Option<PathBuf> {
    let config = load_companion_config();
    if let Some(path) = config.folia_data_dir.as_deref().map(resolve_config_path) {
        if path.exists() {
            return Some(path);
        }
    }
    [
        "~/Library/Application Support/com.laeglaur.folia",
        "~/Library/Application Support/com.laeglaur.notebook",
        "~/Library/Application Support/folia",
    ]
    .iter()
    .map(|path| expand_home_path(path))
    .find(|path| path.join(NOTEBOOK_DATABASE_FILE).exists())
}

fn open_folia_database() -> Result<Connection, String> {
    let data_dir = find_folia_data_dir().ok_or_else(|| {
        "Folia data directory was not found. Install Folia or set foliaDataDir in local/config.json."
            .to_string()
    })?;
    let db_path = data_dir.join(NOTEBOOK_DATABASE_FILE);
    if !db_path.exists() {
        return Err(format!("Folia database not found at {}", db_path.display()));
    }
    Connection::open(db_path).map_err(|error| error.to_string())
}

fn page_exists(connection: &Connection, page_id: &str) -> Result<bool, String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pages WHERE id = ?1",
            params![page_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(count > 0)
}

fn resolve_default_notebook_page_id() -> Result<String, String> {
    let configured = load_companion_config()
        .folia_default_page_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let connection = open_folia_database()?;
    if let Some(page_id) = configured {
        if page_exists(&connection, &page_id)? {
            return Ok(page_id);
        }
        return Err(format!(
            "Configured foliaDefaultPageId was not found: {page_id}"
        ));
    }

    let active_page_id = connection
        .query_row(
            "SELECT active_page_id FROM workspace_preferences WHERE id = 1 LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .filter(|value| !value.trim().is_empty());
    if let Some(page_id) = active_page_id {
        if page_exists(&connection, &page_id)? {
            return Ok(page_id);
        }
    }

    let inbox_page_id = connection
        .query_row(
            "SELECT id FROM pages WHERE lower(title) = 'inbox' ORDER BY rowid LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();
    if let Some(page_id) = inbox_page_id {
        return Ok(page_id);
    }

    connection
        .query_row("SELECT id FROM pages ORDER BY rowid LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| {
            "No Folia page was found. Create a page in Folia or set foliaDefaultPageId in local/config.json."
                .to_string()
        })
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
fn search_notebook_pages(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<NotebookPageSearchResult>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let Some(fts_query) = fts_query_from_search_text(trimmed) else {
        return Ok(Vec::new());
    };
    let connection = open_folia_database()?;
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
    open_notebook_card_for_page(&app, &page_id)
}

fn open_notebook_card_for_page(
    app: &AppHandle,
    page_id: &str,
) -> Result<NotebookCardLaunchResult, String> {
    let request_dir = find_folia_data_dir()
        .map(|dir| dir.join(FOLIA_EXTERNAL_CARD_REQUESTS_DIR))
        .unwrap_or(app_data_dir(app)?.join("folia-card-requests"));
    fs::create_dir_all(&request_dir).map_err(|error| {
        format!(
            "Could not create folia external card request dir {}: {error}",
            request_dir.display()
        )
    })?;
    let request_path = request_dir.join(format!("{}.notecard", request_file_stem()));
    let request = NotebookCardRequest {
        page_id: page_id.to_string(),
    };
    let text = serde_json::to_string(&request)
        .map_err(|error| format!("Could not serialize folia request: {error}"))?;
    fs::write(&request_path, text)
        .map_err(|error| format!("Could not write folia request: {error}"))?;
    if !is_folia_running()? {
        let app_path = find_folia_app().ok_or_else(|| {
            "Folia is not installed or configured. Install Folia or set foliaAppPath in local/config.json."
                .to_string()
        })?;
        Command::new("open")
            .arg("-jg")
            .arg("-a")
            .arg(&app_path)
            .spawn()
            .map_err(|error| format!("Could not launch folia in background: {error}"))?;
    }
    Ok(NotebookCardLaunchResult {
        request_path: request_path.to_string_lossy().to_string(),
    })
}

fn is_folia_running() -> Result<bool, String> {
    let output = Command::new("pgrep")
        .arg("-x")
        .arg("block_first_notebook")
        .output()
        .map_err(|error| format!("Could not check folia process: {error}"))?;
    Ok(output.status.success())
}

#[tauri::command]
fn open_default_notebook_card(app: AppHandle) -> Result<NotebookCardLaunchResult, String> {
    let page_id = resolve_default_notebook_page_id()?;
    open_notebook_card_for_page(&app, &page_id)
}

#[tauri::command]
fn open_notebook_search(app: AppHandle) -> Result<(), String> {
    open_notebook_search_window(&app)
}

#[tauri::command]
fn launch_archive(
    app: AppHandle,
    state: tauri::State<ArchiveProcess>,
) -> Result<ArchiveLaunchResult, String> {
    launch_archive_from_app(&app, &state)
}

fn launch_archive_from_app(
    app: &AppHandle,
    state: &tauri::State<ArchiveProcess>,
) -> Result<ArchiveLaunchResult, String> {
    let archive_app_dir = load_companion_config()
        .archive_app_dir
        .as_deref()
        .map(resolve_config_path)
        .filter(|path| path.exists())
        .ok_or_else(|| {
            "Archive is not configured. Set archiveAppDir in local/config.json to enable Option+Right."
                .to_string()
        })?;
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "Archive process lock is poisoned.".to_string())?;
    if let Some(child) = guard.as_mut() {
        if child
            .try_wait()
            .map_err(|error| format!("Could not inspect Archive process: {error}"))?
            .is_none()
        {
            let port = load_settings(app.clone())?
                .last_archive_port
                .unwrap_or(8765);
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
        .current_dir(&archive_app_dir)
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
    let search_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::KeyF);
    let new_card_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::KeyN);
    append_log(
        app.handle(),
        "registering global shortcuts option+right, option+f, option+n",
    );
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_shortcut(archive_shortcut)?
            .with_shortcut(search_shortcut)?
            .with_shortcut(new_card_shortcut)?
            .with_handler(|app, shortcut, event| {
                append_log(
                    app,
                    &format!(
                        "global shortcut event: option+right={} option+f={} option+n={} state={:?}",
                        shortcut.matches(Modifiers::ALT, Code::ArrowRight),
                        shortcut.matches(Modifiers::ALT, Code::KeyF),
                        shortcut.matches(Modifiers::ALT, Code::KeyN),
                        event.state
                    ),
                );
                if event.state != ShortcutState::Pressed {
                    return;
                }

                if shortcut.matches(Modifiers::ALT, Code::ArrowRight) {
                    append_log(app, "global shortcut option+right: launch archive");
                    let state = app.state::<ArchiveProcess>();
                    if let Err(error) = launch_archive_from_app(app, &state) {
                        append_log(
                            app,
                            &format!("global shortcut launch archive failed: {error}"),
                        );
                    }
                    return;
                }

                if shortcut.matches(Modifiers::ALT, Code::KeyF) {
                    append_log(app, "global shortcut option+f: open notebook search");
                    if let Err(error) = open_notebook_search_window(app) {
                        append_log(
                            app,
                            &format!("global shortcut open notebook search failed: {error}"),
                        );
                    }
                    return;
                }

                if shortcut.matches(Modifiers::ALT, Code::KeyN) {
                    append_log(app, "global shortcut option+n: open default notebook card");
                    if let Err(error) = open_default_notebook_card(app.clone()) {
                        append_log(
                            app,
                            &format!("global shortcut open default notebook card failed: {error}"),
                        );
                    }
                }
            })
            .build(),
    )?;
    append_log(
        app.handle(),
        "registered global shortcuts option+right, option+f, option+n",
    );
    Ok(())
}

fn open_notebook_search_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("notebook-search") {
        window
            .unminimize()
            .map_err(|error| format!("Could not unminimize Folia search window: {error}"))?;
        window
            .show()
            .map_err(|error| format!("Could not show Folia search window: {error}"))?;
        window
            .set_focus()
            .map_err(|error| format!("Could not focus Folia search window: {error}"))?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        app,
        "notebook-search",
        WebviewUrl::App("search.html".into()),
    )
    .title("Search Folia")
    .inner_size(420.0, 300.0)
    .min_inner_size(340.0, 220.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(true)
    .build()
    .map_err(|error| format!("Could not create Folia search window: {error}"))?;
    Ok(())
}

#[tauri::command]
fn reset_position(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("companion") {
        window
            .set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: 60,
                y: 160,
            }))
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
import json
import os
import socketserver
import sys
import urllib.parse
from pathlib import Path

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

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path.endswith(".model3.json"):
            path = Path(self.translate_path(parsed.path))
            if path.exists() and path.is_file():
                try:
                    payload = self.augmented_model_settings(path).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json; charset=utf-8")
                    self.send_header("Content-Length", str(len(payload)))
                    self.end_headers()
                    self.wfile.write(payload)
                    return
                except Exception as error:
                    print(f"model settings augmentation failed: {error}", file=sys.stderr)
        super().do_GET()

    def augmented_model_settings(self, path):
        with path.open("r", encoding="utf-8") as file:
            settings = json.load(file)
        references = settings.setdefault("FileReferences", {})
        root = Path(os.getcwd())
        vtube = self.load_vtube_settings(root)
        hotkeys = vtube.get("Hotkeys", []) if isinstance(vtube, dict) else []

        if not references.get("Expressions"):
            expressions = self.expression_definitions(root, hotkeys)
            if expressions:
                references["Expressions"] = expressions

        if not references.get("Motions"):
            motions = self.motion_definitions(root, hotkeys)
            if motions:
                references["Motions"] = motions

        return json.dumps(settings, ensure_ascii=False, separators=(",", ":"))

    def load_vtube_settings(self, root):
        for vtube_file in sorted(root.glob("*.vtube.json"), key=lambda item: item.name.lower()):
            try:
                with vtube_file.open("r", encoding="utf-8") as file:
                    return json.load(file)
            except Exception:
                return {}
        return {}

    def relative_model_file(self, root, file_name, suffix):
        if not file_name or not file_name.endswith(suffix):
            return None
        direct = root / file_name
        if direct.exists() and direct.is_file():
            return direct.relative_to(root).as_posix()
        matches = sorted(root.rglob(file_name), key=lambda item: item.as_posix().lower())
        for match in matches:
            relative_path = match.relative_to(root)
            if not any(part.startswith(".") for part in relative_path.parts):
                return relative_path.as_posix()
        return None

    def expression_definitions(self, root, hotkeys):
        expressions = []
        seen = set()
        for hotkey in hotkeys:
            if hotkey.get("Action") != "ToggleExpression":
                continue
            relative = self.relative_model_file(root, hotkey.get("File", ""), ".exp3.json")
            if not relative or relative in seen:
                continue
            seen.add(relative)
            name = hotkey.get("Name") or Path(relative).name[:-len(".exp3.json")]
            expressions.append({"Name": name, "File": relative})

        for expression_file in sorted(root.rglob("*.exp3.json"), key=lambda item: item.as_posix().lower()):
            relative_path = expression_file.relative_to(root)
            if any(part.startswith(".") for part in relative_path.parts):
                continue
            relative = relative_path.as_posix()
            if relative in seen:
                continue
            seen.add(relative)
            expressions.append({
                "Name": expression_file.name[:-len(".exp3.json")],
                "File": relative,
            })
        return expressions

    def motion_definitions(self, root, hotkeys):
        motions = {}
        seen = set()
        for hotkey in hotkeys:
            if hotkey.get("Action") != "TriggerAnimation":
                continue
            relative = self.relative_model_file(root, hotkey.get("File", ""), ".motion3.json")
            if not relative or relative in seen:
                continue
            seen.add(relative)
            group = Path(relative).name[:-len(".motion3.json")]
            motions.setdefault(group, []).append({"File": relative})

        for motion_file in sorted(root.rglob("*.motion3.json"), key=lambda item: item.as_posix().lower()):
            relative_path = motion_file.relative_to(root)
            if any(part.startswith(".") for part in relative_path.parts):
                continue
            relative = relative_path.as_posix()
            if relative in seen:
                continue
            seen.add(relative)
            group = motion_file.name[:-len(".motion3.json")]
            motions.setdefault(group, []).append({"File": relative})
        return motions

socketserver.ThreadingTCPServer.allow_reuse_address = True
with socketserver.ThreadingTCPServer(("127.0.0.1", int(sys.argv[1])), Handler) as httpd:
    print(httpd.server_address[1], flush=True)
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

fn discover_reactions_in_dir(root: &Path) -> Vec<CompanionReaction> {
    let mut reactions = discover_vtube_hotkey_reactions(root);
    for file in collect_files_with_suffix(root, ".motion3.json") {
        let name = model_asset_stem(&file, ".motion3.json");
        reactions.push(CompanionReaction {
            kind: "motion".to_string(),
            name: name.clone(),
            group: Some(name),
            expression: None,
        });
    }
    for file in collect_files_with_suffix(root, ".exp3.json") {
        let name = model_asset_stem(&file, ".exp3.json");
        reactions.push(CompanionReaction {
            kind: "expression".to_string(),
            name: name.clone(),
            group: None,
            expression: Some(name),
        });
    }
    dedup_reactions(reactions)
}

fn discover_vtube_hotkey_reactions(root: &Path) -> Vec<CompanionReaction> {
    let Some(vtube) = read_first_vtube_settings(root) else {
        return Vec::new();
    };
    let Some(hotkeys) = vtube.get("Hotkeys").and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    let mut reactions = Vec::new();
    for hotkey in hotkeys {
        let action = hotkey
            .get("Action")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let file = hotkey
            .get("File")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let hotkey_name = hotkey
            .get("Name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        match action {
            "TriggerAnimation" => {
                if relative_model_file(root, file, ".motion3.json").is_some() {
                    let group = file.trim_end_matches(".motion3.json").to_string();
                    reactions.push(CompanionReaction {
                        kind: "motion".to_string(),
                        name: if hotkey_name.is_empty() {
                            group.clone()
                        } else {
                            hotkey_name.to_string()
                        },
                        group: Some(group),
                        expression: None,
                    });
                }
            }
            "ToggleExpression" => {
                if let Some(expression_file) = relative_model_file(root, file, ".exp3.json") {
                    let expression = model_asset_stem(&expression_file, ".exp3.json");
                    let name = if hotkey_name.is_empty() {
                        expression.clone()
                    } else {
                        hotkey_name.to_string()
                    };
                    reactions.push(CompanionReaction {
                        kind: "expression".to_string(),
                        name,
                        group: None,
                        expression: Some(expression),
                    });
                }
            }
            "RemoveAllExpressions" => {
                reactions.push(CompanionReaction {
                    kind: "clear".to_string(),
                    name: "clear expressions".to_string(),
                    group: None,
                    expression: None,
                });
            }
            _ => {}
        }
    }
    reactions
}

fn read_first_vtube_settings(root: &Path) -> Option<serde_json::Value> {
    let mut entries = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| name.ends_with(".vtube.json"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort();
    let path = entries.first()?;
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn relative_model_file(root: &Path, file_name: &str, suffix: &str) -> Option<PathBuf> {
    if file_name.is_empty() || !file_name.ends_with(suffix) {
        return None;
    }
    let direct = root.join(file_name);
    if direct.is_file() {
        return Some(PathBuf::from(file_name));
    }
    collect_files_with_suffix(root, suffix)
        .into_iter()
        .find(|path| path.file_name().and_then(|value| value.to_str()) == Some(file_name))
}

fn collect_files_with_suffix(root: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_with_suffix_into(root, root, suffix, &mut files);
    files.sort();
    files
}

fn collect_files_with_suffix_into(
    root: &Path,
    path: &Path,
    suffix: &str,
    files: &mut Vec<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            collect_files_with_suffix_into(root, &path, suffix, files);
            continue;
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.ends_with(suffix))
            .unwrap_or(false)
        {
            files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
}

fn model_asset_stem(path: &Path, suffix: &str) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .trim_end_matches(suffix)
        .to_string()
}

fn dedup_reactions(reactions: Vec<CompanionReaction>) -> Vec<CompanionReaction> {
    let mut result = Vec::new();
    for reaction in reactions {
        let exists = result.iter().any(|existing: &CompanionReaction| {
            if existing.kind != reaction.kind {
                return false;
            }
            match existing.kind.as_str() {
                "motion" => existing.group == reaction.group,
                "expression" => existing.expression == reaction.expression,
                _ => existing.name == reaction.name,
            }
        });
        if !exists {
            result.push(reaction);
        }
    }
    result
}

fn discover_models_from_config(config: &CompanionConfig) -> Vec<CompanionModel> {
    let default_model = config
        .default_model_path
        .as_deref()
        .map(resolve_config_path);
    let roots = config
        .live2d_roots
        .iter()
        .map(|root| resolve_config_path(root))
        .collect::<Vec<_>>();
    discover_models_in_roots(&roots, default_model.as_deref())
}

fn discover_models_in_roots(
    roots: &[PathBuf],
    default_model: Option<&Path>,
) -> Vec<CompanionModel> {
    let mut collected = Vec::new();
    for root in roots {
        collect_model_files(root, "local", default_model, &mut collected);
    }

    let mut by_folder: HashMap<String, CompanionModel> = HashMap::new();
    for model in collected {
        by_folder
            .entry(model.folder.clone())
            .and_modify(|existing| {
                if model_preference_score(&model) > model_preference_score(existing) {
                    *existing = model.clone();
                }
            })
            .or_insert(model);
    }

    let mut models = by_folder.into_values().collect::<Vec<_>>();
    models.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
            .then_with(|| a.model_path.cmp(&b.model_path))
    });
    models.dedup_by(|a, b| a.model_path == b.model_path);
    models
}

fn model_preference_score(model: &CompanionModel) -> i32 {
    if model.is_default {
        return 10_000;
    }

    let display_name = model.display_name.to_lowercase();
    let normalized_name = normalize_model_name(&display_name);
    let folder_name = Path::new(&model.folder)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    let normalized_folder = normalize_model_name(&folder_name);
    let mut score = normalized_name.len().min(40) as i32;

    if normalized_name == normalized_folder {
        score += 80;
    } else if !normalized_folder.is_empty() && normalized_name.contains(&normalized_folder) {
        score += 40;
    } else if let Some(token) = normalized_folder
        .split(|character: char| !character.is_ascii_alphanumeric())
        .find(|token| token.len() > 2)
    {
        if normalized_name.contains(token) {
            score += 20;
        }
    }

    if normalized_name == "model" {
        score -= 100;
    }
    if normalized_name.starts_with("modelv") || normalized_name.ends_with("v1") {
        score -= 80;
    }
    if normalized_name.contains("copy") || normalized_name.contains("backup") {
        score -= 50;
    }

    score
}

fn normalize_model_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn collect_model_files(
    root: &Path,
    source_group: &str,
    default_model: Option<&Path>,
    models: &mut Vec<CompanionModel>,
) {
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
        let is_default = default_model
            .map(|default_path| path == default_path)
            .unwrap_or(false);
        let id = model_id_from_path(root, &path);
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

fn model_id_from_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
            is_bounds_calibration_mode,
            write_precomputed_bounds,
            load_local_bounds,
            write_local_bounds,
            log_event,
            discover_models,
            model_reactions,
            model_asset_url,
            open_notebook,
            open_icity_login,
            search_notebook_pages,
            open_notebook_card,
            open_default_notebook_card,
            open_notebook_search,
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

        let models = discover_models_in_roots(
            &[default_dir.clone(), anime_dir.clone()],
            Some(default_model.as_path()),
        );

        assert_eq!(models.len(), 2);
        assert!(models[0].is_default);
        assert_eq!(models[0].display_name, "huohuo");
        assert!(models.iter().any(|model| model.display_name == "dog"));
    }

    #[test]
    fn keeps_one_preferred_model_per_folder() {
        let temp = tempfile::tempdir().unwrap();
        let default_dir = temp.path().join("huohuo");
        let anime_dir = temp.path().join("anime");
        let nagito_dir = anime_dir.join("Nagito vtuber model");
        fs::create_dir_all(&default_dir).unwrap();
        fs::create_dir_all(&nagito_dir).unwrap();
        let default_model = default_dir.join("huohuo.model3.json");
        fs::write(&default_model, "{}").unwrap();
        fs::write(nagito_dir.join("NagitoModel.model3.json"), "{}").unwrap();
        fs::write(nagito_dir.join("modelv1.model3.json"), "{}").unwrap();

        let models = discover_models_in_roots(
            &[default_dir.clone(), anime_dir.clone()],
            Some(default_model.as_path()),
        );

        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .any(|model| model.display_name == "NagitoModel"));
        assert!(!models.iter().any(|model| model.display_name == "modelv1"));
    }

    #[test]
    fn vtube_expression_reaction_uses_model_expression_name() {
        let temp = tempfile::tempdir().unwrap();
        let model_dir = temp.path().join("cat");
        let expression_dir = model_dir.join("expressions");
        fs::create_dir_all(&expression_dir).unwrap();
        fs::write(expression_dir.join("kowai.exp3.json"), "{}").unwrap();
        fs::write(
            model_dir.join("cat.vtube.json"),
            r#"{
                "Hotkeys": [
                    {
                        "Name": "怖い顔",
                        "Action": "ToggleExpression",
                        "File": "kowai.exp3.json"
                    }
                ]
            }"#,
        )
        .unwrap();

        let reactions = discover_reactions_in_dir(&model_dir);

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, "expression");
        assert_eq!(reactions[0].name, "怖い顔");
        assert_eq!(reactions[0].expression.as_deref(), Some("kowai"));
    }

    #[test]
    fn vtube_reactions_include_files_missing_from_hotkeys() {
        let temp = tempfile::tempdir().unwrap();
        let model_dir = temp.path().join("huohuo");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("qizi.motion3.json"), "{}").unwrap();
        fs::write(model_dir.join("Scene1.motion3.json"), "{}").unwrap();
        fs::write(model_dir.join("qizi1.exp3.json"), "{}").unwrap();
        fs::write(
            model_dir.join("huohuo.vtube.json"),
            r#"{
                "Hotkeys": [
                    {
                        "Name": "",
                        "Action": "TriggerAnimation",
                        "File": "qizi.motion3.json"
                    },
                    {
                        "Name": "qizi1",
                        "Action": "ToggleExpression",
                        "File": "qizi1.exp3.json"
                    }
                ]
            }"#,
        )
        .unwrap();

        let reactions = discover_reactions_in_dir(&model_dir);
        let keys: Vec<String> = reactions
            .iter()
            .map(|reaction| match reaction.kind.as_str() {
                "motion" => format!("motion:{}", reaction.group.as_deref().unwrap_or("")),
                "expression" => format!(
                    "expression:{}",
                    reaction.expression.as_deref().unwrap_or("")
                ),
                _ => reaction.kind.clone(),
            })
            .collect();

        assert!(keys.contains(&"motion:qizi".to_string()));
        assert!(keys.contains(&"motion:Scene1".to_string()));
        assert!(keys.contains(&"expression:qizi1".to_string()));
        assert_eq!(
            keys.iter()
                .filter(|key| key.as_str() == "motion:qizi")
                .count(),
            1
        );
    }

    #[test]
    fn preferred_port_is_selected_when_free() {
        let port = choose_port(9876).unwrap();
        assert_eq!(port, 9876);
    }
}
