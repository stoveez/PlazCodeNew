use crate::{mcp_addons, preferences};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::time::{Duration, Instant};
#[cfg(windows)]
use tao::dpi::LogicalSize;
#[cfg(windows)]
use tao::event::{Event, WindowEvent};
#[cfg(windows)]
use tao::event_loop::{ControlFlow, EventLoopBuilder};
#[cfg(windows)]
use tao::platform::run_return::EventLoopExtRunReturn;
#[cfg(windows)]
use tao::window::{Icon, WindowBuilder};
#[cfg(windows)]
use wry::WebViewBuilder;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ToolDisplay {
    pub name: String,
    pub source: String,
    pub available: bool,
}

pub struct UiShared {
    pub studio_running: AtomicBool,
    pub mcp_alive: Arc<AtomicBool>,
    pub workspace_ready: AtomicBool,
    pub full_access: std::sync::RwLock<Arc<AtomicBool>>,
    pub workspace_root: Mutex<String>,
    pub fatal: Mutex<Option<String>>,
    pub logs: Mutex<VecDeque<String>>,
    pub extension_seen_ms: std::sync::atomic::AtomicU64,
    pub show_requested: AtomicBool,
    pub quit_requested: AtomicBool,
    pub tools: Mutex<Vec<ToolDisplay>>,
    pub servers: Mutex<Vec<mcp_addons::ServerSummary>>,
}

const LOG_CAP: usize = 1200;

impl UiShared {
    pub fn new(mcp_alive: Arc<AtomicBool>) -> Self {
        Self {
            studio_running: AtomicBool::new(false),
            mcp_alive,
            workspace_ready: AtomicBool::new(false),
            full_access: std::sync::RwLock::new(Arc::new(AtomicBool::new(false))),
            workspace_root: Mutex::new(String::new()),
            fatal: Mutex::new(None),
            logs: Mutex::new(VecDeque::with_capacity(LOG_CAP)),
            extension_seen_ms: std::sync::atomic::AtomicU64::new(0),
            show_requested: AtomicBool::new(false),
            quit_requested: AtomicBool::new(false),
            tools: Mutex::new(Vec::new()),
            servers: Mutex::new(Vec::new()),
        }
    }

    pub fn attach_workspace(&self, root: String, full_flag: Arc<AtomicBool>) {
        if let Ok(mut value) = self.workspace_root.lock() {
            *value = root;
        }
        self.workspace_ready.store(true, Ordering::Relaxed);
        let current = self.full_access.read()
            .map(|flag| flag.load(Ordering::Relaxed))
            .unwrap_or(false);
        full_flag.store(current, Ordering::Relaxed);
        if let Ok(mut slot) = self.full_access.write() {
            *slot = full_flag;
        }
    }

    pub fn log(&self, chunk: &str) {
        let mut logs = self.logs.lock().unwrap_or_else(|error| error.into_inner());
        for line in chunk.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if logs.len() == LOG_CAP {
                logs.pop_front();
            }
            logs.push_back(trimmed.to_string());
        }
    }

    pub fn clear_logs(&self) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
    }

    pub fn mark_extension_seen(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.extension_seen_ms.store(now, Ordering::Relaxed);
    }

    pub fn extension_recent(&self) -> bool {
        let seen = self.extension_seen_ms.load(Ordering::Relaxed);
        if seen == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(seen) < 15_000
    }

    pub fn request_show(&self) {
        self.show_requested.store(true, Ordering::Relaxed);
    }

    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::Relaxed);
    }

    pub fn set_fatal(&self, msg: String) {
        let mut fatal = self.fatal.lock().unwrap_or_else(|error| error.into_inner());
        if fatal.is_none() {
            *fatal = Some(msg);
        }
    }

    fn replace_source_tools(&self, source: &str, tools: &[serde_json::Value], available: bool) {
        if let Ok(mut rows) = self.tools.lock() {
            rows.retain(|row| row.source != source);
            rows.extend(tools.iter().filter_map(|tool| {
                tool.get("name").and_then(|value| value.as_str()).map(|name| ToolDisplay {
                    name: name.to_string(),
                    source: source.to_string(),
                    available,
                })
            }));
            rows.sort_by(|left, right| left.source.cmp(&right.source).then_with(|| left.name.cmp(&right.name)));
            rows.dedup_by(|left, right| left.source == right.source && left.name == right.name);
        }
    }

    pub fn set_roblox_tools(&self, tools: &[serde_json::Value], available: bool) {
        self.replace_source_tools("Roblox Studio", tools, available);
    }

    pub fn set_local_tools(&self, tools: &[serde_json::Value], available: bool) {
        self.replace_source_tools("AgentScript", tools, available);
    }

    pub fn set_addon_tools(&self, tools: &[serde_json::Value], servers: Vec<mcp_addons::ServerSummary>) {
        if let Ok(mut rows) = self.tools.lock() {
            rows.retain(|row| !row.source.starts_with("MCP / "));
            for tool in tools {
                let Some(name) = tool.get("name").and_then(|value| value.as_str()) else { continue; };
                let server = tool.get("server").and_then(|value| value.as_str())
                    .or_else(|| name.split_once("__").map(|parts| parts.0))
                    .unwrap_or("addon");
                rows.push(ToolDisplay {
                    name: name.to_string(),
                    source: format!("MCP / {server}"),
                    available: true,
                });
            }
            rows.sort_by(|left, right| left.source.cmp(&right.source).then_with(|| left.name.cmp(&right.name)));
            rows.dedup_by(|left, right| left.source == right.source && left.name == right.name);
        }
        if let Ok(mut current) = self.servers.lock() {
            *current = servers;
        }
    }

    pub fn set_browser_tools(&self, tools: &[serde_json::Value]) {
        if let Ok(mut rows) = self.tools.lock() {
            rows.retain(|row| !row.source.starts_with("PlazCode"));
            for tool in tools {
                let Some(name) = tool.get("name").and_then(|value| value.as_str()) else { continue; };
                let source = tool.get("source").and_then(|value| value.as_str()).unwrap_or("PlazCode");
                rows.push(ToolDisplay {
                    name: name.to_string(),
                    source: source.to_string(),
                    available: true,
                });
            }
            rows.sort_by(|left, right| left.source.cmp(&right.source).then_with(|| left.name.cmp(&right.name)));
            rows.dedup_by(|left, right| left.source == right.source && left.name == right.name);
        }
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
enum UiEvent {
    Hide,
    Quit,
    Minimize,
    Maximize,
    Drag,
}

#[cfg(windows)]
fn window_icon() -> anyhow::Result<Icon> {
    let width = 64usize;
    let height = 64usize;
    let mut rgba = vec![0u8; width * height * 4];

    let set = |rgba: &mut [u8], x: usize, y: usize, color: [u8; 4]| {
        if x >= width || y >= height {
            return;
        }
        let index = (y * width + x) * 4;
        rgba[index..index + 4].copy_from_slice(&color);
    };

    for y in 3..61 {
        for x in 3..61 {
            let dx = if x < 14 { 14 - x } else if x > 50 { x - 50 } else { 0 };
            let dy = if y < 14 { 14 - y } else if y > 50 { y - 50 } else { 0 };
            if dx * dx + dy * dy <= 11 * 11 {
                set(&mut rgba, x, y, [6, 20, 38, 255]);
            }
        }
    }

    let orange = [255, 164, 48, 255];
    for y in 14..40 {
        for x in 17..23 {
            set(&mut rgba, x, y, orange);
        }
    }
    for y in 14..20 {
        for x in 22..42 {
            set(&mut rgba, x, y, orange);
        }
    }
    for y in 29..35 {
        for x in 22..39 {
            set(&mut rgba, x, y, orange);
        }
    }
    for y in 16..34 {
        let dy = y as i32 - 25;
        for x in 34..48 {
            let dx = x as i32 - 36;
            let outer = dx * dx + dy * dy <= 12 * 12;
            let inner = dx * dx + dy * dy <= 6 * 6;
            if outer && !inner {
                set(&mut rgba, x, y, orange);
            }
        }
    }
    for y in 43..55 {
        let offset = (y - 43) / 2;
        for x in (18usize.saturating_sub(offset))..(24usize.saturating_sub(offset)) {
            set(&mut rgba, x, y, orange);
        }
        for x in (30usize.saturating_sub(offset))..(36usize.saturating_sub(offset)) {
            set(&mut rgba, x, y, orange);
        }
    }

    Icon::from_rgba(rgba, width as u32, height as u32)
        .map_err(|error| anyhow::anyhow!("invalid PlazCode window icon: {error}"))
}

#[cfg(windows)]
fn wait_for_desktop_server() {
    for _ in 0..40 {
        if std::net::TcpStream::connect("127.0.0.1:3000").is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
pub fn run_gui(
    shared: Arc<UiShared>,
    _restart_tx: tokio::sync::mpsc::UnboundedSender<()>,
    _preferences: Arc<preferences::PreferencesStore>,
) -> anyhow::Result<()> {
    let pairing_key = crate::security::load_key()?;
    wait_for_desktop_server();

    let mut event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title(format!("PlazCode {}", env!("CARGO_PKG_VERSION")))
        .with_decorations(false)
        .with_resizable(true)
        .with_inner_size(LogicalSize::new(1460.0, 900.0))
        .with_min_inner_size(LogicalSize::new(900.0, 620.0))
        .with_window_icon(Some(window_icon()?))
        .build(&event_loop)
        .map_err(|error| anyhow::anyhow!("cannot create PlazCode window: {error}"))?;

    let proxy = event_loop.create_proxy();
    let ipc_proxy = proxy.clone();
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_str(&format!("Bearer {pairing_key}"))?,
    );

    let _webview = WebViewBuilder::new()
        .with_url_and_headers("http://127.0.0.1:3000/desktop", headers)
        .with_ipc_handler(move |request| {
            let event = match request.body().as_str() {
                "hide" => Some(UiEvent::Hide),
                "quit" => Some(UiEvent::Quit),
                "minimize" => Some(UiEvent::Minimize),
                "maximize" => Some(UiEvent::Maximize),
                "drag" => Some(UiEvent::Drag),
                _ => None,
            };
            if let Some(event) = event {
                let _ = ipc_proxy.send_event(event);
            }
        })
        .with_devtools(cfg!(debug_assertions))
        .build(&window)
        .map_err(|error| anyhow::anyhow!("cannot create PlazCode WebView: {error}"))?;

    window.set_visible(true);
    window.set_focus();

    let mut last_tick = Instant::now();
    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));

        match event {
            Event::UserEvent(UiEvent::Hide) => window.set_visible(false),
            Event::UserEvent(UiEvent::Quit) => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UiEvent::Minimize) => window.set_minimized(true),
            Event::UserEvent(UiEvent::Maximize) => window.set_maximized(!window.is_maximized()),
            Event::UserEvent(UiEvent::Drag) => {
                let _ = window.drag_window();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                window.set_visible(false);
                shared.log("desktop window hidden — PlazCode is still running in the background");
            }
            Event::MainEventsCleared => {
                if last_tick.elapsed() >= Duration::from_millis(100) {
                    last_tick = Instant::now();
                    if shared.show_requested.swap(false, Ordering::Relaxed) {
                        window.set_visible(true);
                        window.set_minimized(false);
                        window.set_focus();
                    }
                    if shared.quit_requested.swap(false, Ordering::Relaxed) {
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            _ => {}
        }
    });

    Ok(())
}

#[cfg(not(windows))]
pub fn run_gui(
    _shared: Arc<UiShared>,
    _restart_tx: tokio::sync::mpsc::UnboundedSender<()>,
    _preferences: Arc<preferences::PreferencesStore>,
) -> anyhow::Result<()> {
    anyhow::bail!("PlazCode desktop UI currently requires Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(rows: &[ToolDisplay]) -> Vec<(String, String)> {
        rows.iter().map(|row| (row.name.clone(), row.source.clone())).collect()
    }

    #[test]
    fn tool_catalog_merges_all_sources() {
        let shared = UiShared::new(Arc::new(AtomicBool::new(false)));

        shared.set_roblox_tools(
            &[serde_json::json!({"name":"execute_luau"})],
            true,
        );
        shared.set_local_tools(
            &[serde_json::json!({"name":"read_file"})],
            true,
        );
        shared.set_addon_tools(
            &[serde_json::json!({"name":"memory__search","server":"memory"})],
            Vec::new(),
        );
        shared.set_browser_tools(
            &[
                serde_json::json!({"name":"web_search","source":"PlazCode"}),
                serde_json::json!({"name":"animation_create","source":"PlazCode / Motion"}),
            ],
        );

        let rows = shared.tools.lock().expect("tool lock").clone();
        let values = names(&rows);
        assert!(values.contains(&("execute_luau".to_string(), "Roblox Studio".to_string())));
        assert!(values.contains(&("read_file".to_string(), "AgentScript".to_string())));
        assert!(values.contains(&("memory__search".to_string(), "MCP / memory".to_string())));
        assert!(values.contains(&("web_search".to_string(), "PlazCode".to_string())));
        assert!(values.contains(&("animation_create".to_string(), "PlazCode / Motion".to_string())));
    }

    #[test]
    fn browser_tool_refresh_replaces_only_browser_rows() {
        let shared = UiShared::new(Arc::new(AtomicBool::new(false)));
        shared.set_local_tools(&[serde_json::json!({"name":"tree"})], true);
        shared.set_browser_tools(&[serde_json::json!({"name":"web_search","source":"PlazCode"})]);
        shared.set_browser_tools(&[serde_json::json!({"name":"web_fetch","source":"PlazCode"})]);

        let rows = shared.tools.lock().expect("tool lock").clone();
        assert!(rows.iter().any(|row| row.name == "tree" && row.source == "AgentScript"));
        assert!(rows.iter().any(|row| row.name == "web_fetch" && row.source == "PlazCode"));
        assert!(!rows.iter().any(|row| row.name == "web_search" && row.source == "PlazCode"));
    }
}
