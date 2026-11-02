// 避免在 Windows 獨立執行檔案執行時彈出黑底 Console 視窗，只開啟桌面介面
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::dispatch_command;
use rust_embed::RustEmbed;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Response, Server};

use tao::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

#[cfg(windows)]
fn try_attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        // 若由現有終端機（如 PowerShell / CMD）啟動，附掛到父行程終端機以便輸出日誌
        // 若由檔案總管雙擊啟動，父行程為 explorer.exe，此函式安全返回失敗，不會開啟任何黑底 console 視窗
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(RustEmbed)]
#[folder = "../ui/dist"]
pub struct EmbeddedUi;

enum StaticPayload {
    Bytes(Vec<u8>),
    Cow(std::borrow::Cow<'static, [u8]>),
}

impl StaticPayload {
    fn as_slice(&self) -> &[u8] {
        match self {
            StaticPayload::Bytes(b) => b.as_slice(),
            StaticPayload::Cow(c) => c.as_ref(),
        }
    }
}

pub struct StaticResource {
    payload: StaticPayload,
    content_type: &'static str,
}

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        Header::from_bytes(
            &b"Access-Control-Allow-Methods"[..],
            &b"GET, POST, OPTIONS"[..],
        )
        .unwrap(),
        Header::from_bytes(
            &b"Access-Control-Allow-Headers"[..],
            &b"Content-Type"[..],
        )
        .unwrap(),
    ]
}

pub fn content_type_for_path_str(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or_default();
    match ext.to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "json" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn locate_ui_dist() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("ui/dist"),
        PathBuf::from("../ui/dist"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("ui/dist")))
            .unwrap_or_else(|| PathBuf::from("ui/dist")),
    ];
    candidates.into_iter().find(|p| p.join("index.html").exists())
}

pub fn resolve_static_resource(url_path: &str, dist_dir: Option<&Path>) -> Option<StaticResource> {
    let clean_rel = url_path.trim_start_matches('/').split('?').next().unwrap_or("");
    let target = if clean_rel.is_empty() { "index.html" } else { clean_rel };

    // 1. 若外部 dist_dir 目錄存在，優先從磁碟讀取（便於本機熱開發與抽換）
    if let Some(dist) = dist_dir {
        let candidate = dist.join(target);
        if candidate.is_file() {
            if let Ok(bytes) = fs::read(&candidate) {
                return Some(StaticResource {
                    payload: StaticPayload::Bytes(bytes),
                    content_type: content_type_for_path_str(target),
                });
            }
        }
    }

    // 2. 從單獨執行檔內嵌的 EmbeddedUi 讀取
    if let Some(file) = EmbeddedUi::get(target) {
        return Some(StaticResource {
            payload: StaticPayload::Cow(file.data),
            content_type: content_type_for_path_str(target),
        });
    }

    // 3. SPA 路由 Fallback 到 index.html (若請求沒有副檔名或副檔名為 html)
    if !target.contains('.') || target.ends_with(".html") {
        if let Some(dist) = dist_dir {
            let index_path = dist.join("index.html");
            if index_path.is_file() {
                if let Ok(bytes) = fs::read(&index_path) {
                    return Some(StaticResource {
                        payload: StaticPayload::Bytes(bytes),
                        content_type: "text/html; charset=utf-8",
                    });
                }
            }
        }
        if let Some(file) = EmbeddedUi::get("index.html") {
            return Some(StaticResource {
                payload: StaticPayload::Cow(file.data),
                content_type: "text/html; charset=utf-8",
            });
        }
    }

    None
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_maximized: bool,
}

fn get_window_state_path() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = Path::new(&appdata).join("my-computer-keeper");
        let _ = fs::create_dir_all(&dir);
        Some(dir.join("window-state.json"))
    } else {
        None
    }
}

fn load_window_state() -> Option<WindowState> {
    let path = get_window_state_path()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_window_state(state: &WindowState) {
    if let Some(path) = get_window_state_path() {
        if let Ok(json) = serde_json::to_string_pretty(state) {
            let _ = fs::write(path, json);
        }
    }
}

fn is_in_screen_bounds(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    monitors: impl Iterator<Item = tao::monitor::MonitorHandle>,
) -> bool {
    let win_right = x.saturating_add(width as i32);
    let win_bottom = y.saturating_add(height as i32);

    for m in monitors {
        let m_pos = m.position();
        let m_size = m.size();
        let m_left = m_pos.x;
        let m_top = m_pos.y;
        let m_right = m_left.saturating_add(m_size.width as i32);
        let m_bottom = m_top.saturating_add(m_size.height as i32);

        let inter_left = x.max(m_left);
        let inter_top = y.max(m_top);
        let inter_right = win_right.min(m_right);
        let inter_bottom = win_bottom.min(m_bottom);

        if inter_right > inter_left && inter_bottom > inter_top {
            let inter_width = (inter_right - inter_left) as u32;
            let inter_height = (inter_bottom - inter_top) as u32;
            if inter_width >= 200 && inter_height >= 100 {
                return true;
            }
        }
    }
    false
}

fn run_desktop_window(app_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new();
    let saved_state = load_window_state();

    let in_bounds = if let Some(ref st) = saved_state {
        is_in_screen_bounds(st.x, st.y, st.width, st.height, event_loop.available_monitors())
    } else {
        false
    };

    let mut builder = WindowBuilder::new()
        .with_title("My Computer Keeper — 全能電腦維運與檔案管家")
        .with_min_inner_size(LogicalSize::new(1100.0, 720.0));

    if let Some(st) = saved_state {
        if in_bounds {
            builder = builder
                .with_position(PhysicalPosition::new(st.x, st.y))
                .with_inner_size(PhysicalSize::new(st.width.max(1100), st.height.max(720)))
                .with_maximized(st.is_maximized);
        } else {
            // 若不在既有螢幕範圍，則回歸主螢幕最大化呈現
            builder = builder
                .with_inner_size(LogicalSize::new(1440.0, 920.0))
                .with_maximized(true);
        }
    } else {
        builder = builder
            .with_inner_size(LogicalSize::new(1440.0, 920.0));
    }

    let window = builder.build(&event_loop)?;

    let _webview = WebViewBuilder::new()
        .with_url(app_url)
        .with_devtools(cfg!(debug_assertions))
        .build(&window)?;

    let mut last_normal_pos = window.outer_position().unwrap_or(PhysicalPosition::new(100, 100));
    let mut last_normal_size = window.inner_size();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::Moved(pos), .. } => {
                if !window.is_maximized() {
                    last_normal_pos = pos;
                }
            }
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                if !window.is_maximized() && size.width > 0 && size.height > 0 {
                    last_normal_size = size;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if keeper_core::vhdx::compact_vhdx_status().state == keeper_core::vhdx::VhdxCompactState::Running {
                    window.set_title("VHDX workflow running — please wait before closing");
                } else {
                    let is_max = window.is_maximized();
                    let (pos, size) = if is_max {
                        (last_normal_pos, last_normal_size)
                    } else {
                        (
                            window.outer_position().unwrap_or(last_normal_pos),
                            window.inner_size(),
                        )
                    };

                    save_window_state(&WindowState {
                        x: pos.x,
                        y: pos.y,
                        width: size.width.max(1100),
                        height: size.height.max(720),
                        is_maximized: is_max,
                    });

                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

fn launch_browser_fallback(url: &str) {
    #[cfg(windows)]
    {
        let edge_paths = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        for edge in edge_paths {
            if Path::new(edge).exists() {
                let _ = Command::new(edge)
                    .arg(format!("--app={}", url))
                    .arg("--window-size=1440,920")
                    .spawn();
                return;
            }
        }

        // 使用 Windows 原生 API ShellExecuteW 安全開啟預設瀏覽器，不呼叫 cmd.exe 以免觸發防毒軟體啟發式誤判
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide_url: Vec<u16> = OsStr::new(url).encode_wide().chain(std::iter::once(0)).collect();
        let wide_open: Vec<u16> = OsStr::new("open").encode_wide().chain(std::iter::once(0)).collect();

        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                wide_open.as_ptr(),
                wide_url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

fn run_http_service(
    server: Arc<Server>,
    ui_dist: Option<PathBuf>,
    shutdown_pending: Arc<AtomicBool>,
    has_connected: Arc<AtomicBool>,
    last_activity: Arc<AtomicU64>,
) {
    for mut request in server.incoming_requests() {
        last_activity.store(current_timestamp_secs(), Ordering::SeqCst);

        if *request.method() == Method::Options {
            let mut resp = Response::empty(204);
            for h in cors_headers() {
                resp.add_header(h);
            }
            let _ = request.respond(resp);
            continue;
        }

        let url = request.url().to_string();
        if let Some(cmd_part) = url.strip_prefix("/api/") {
            let cmd = cmd_part.split('?').next().unwrap_or_default();

            // 前端定時心跳 ping
            if cmd == "heartbeat" {
                shutdown_pending.store(false, Ordering::SeqCst);
                has_connected.store(true, Ordering::SeqCst);
                let mut resp = Response::from_string("{\"ok\":true}");
                resp.add_header(
                    Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json; charset=utf-8"[..],
                    )
                    .unwrap(),
                );
                for h in cors_headers() {
                    resp.add_header(h);
                }
                let _ = request.respond(resp);
                continue;
            }

            // 前端視窗關閉或使用者點擊結束程式
            if cmd == "shutdown" {
                let running = keeper_core::vhdx::compact_vhdx_status().state == keeper_core::vhdx::VhdxCompactState::Running;
                shutdown_pending.store(!running, Ordering::SeqCst);
                let mut resp = Response::from_string(if running {
                    "{\"ok\":false,\"error\":\"VHDX workflow is running\"}"
                } else {
                    "{\"ok\":true}"
                }).with_status_code(if running { 409 } else { 200 });
                resp.add_header(
                    Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json; charset=utf-8"[..],
                    )
                    .unwrap(),
                );
                for h in cors_headers() {
                    resp.add_header(h);
                }
                let _ = request.respond(resp);
                continue;
            }

            shutdown_pending.store(false, Ordering::SeqCst);
            has_connected.store(true, Ordering::SeqCst);

            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let payload: Value = if body.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&body).unwrap_or_else(|_| json!({}))
            };

            let result_val = dispatch_command(cmd, &payload);
            let serialized = serde_json::to_string(&result_val).unwrap_or_else(|_| "{}".to_string());

            let mut resp = Response::from_string(serialized);
            resp.add_header(
                Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"application/json; charset=utf-8"[..],
                )
                .unwrap(),
            );
            for h in cors_headers() {
                resp.add_header(h);
            }
            let _ = request.respond(resp);
            continue;
        }

        // 提供前端靜態檔案服務 (支援單獨執行檔內嵌資源與本機檔案服務)
        shutdown_pending.store(false, Ordering::SeqCst);
        has_connected.store(true, Ordering::SeqCst);

        if let Some(res) = resolve_static_resource(&url, ui_dist.as_deref()) {
            let mut resp = Response::from_data(res.payload.as_slice().to_vec());
            resp.add_header(
                Header::from_bytes(&b"Content-Type"[..], res.content_type.as_bytes()).unwrap(),
            );
            for h in cors_headers() {
                resp.add_header(h);
            }
            let _ = request.respond(resp);
            continue;
        }

        let fallback = Response::from_string(
            "My Computer Keeper Rust RPC Server is running. UI assets not found.",
        );
        let _ = request.respond(fallback);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // UAC 提權後的虛擬磁碟壓縮子行程：執行完即結束，不啟動 UI 服務
    if args.get(1).map(String::as_str) == Some(keeper_core::vhdx::COMPACT_WORKER_ARG) {
        let vhdx = args.get(2).map(String::as_str).unwrap_or_default();
        let result_file = args.get(3).map(String::as_str).unwrap_or_default();
        let progress_file = args.get(4).map(String::as_str).unwrap_or_default();
        std::process::exit(keeper_core::vhdx::run_compact_worker(vhdx, result_file, progress_file));
    }

    #[cfg(windows)]
    try_attach_console();

    let headless = args.iter().any(|a| a == "--headless" || a == "--api-only");
    let bind_target = std::env::var("KEEPER_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:14280".to_string());

    let server = match Server::http(&bind_target) {
        Ok(s) => s,
        Err(_) => {
            // 若預設連接埠被佔用，自動指派本機隨機空閒通訊埠
            Server::http("127.0.0.1:0").expect("Failed to bind to local port")
        }
    };

    let actual_addr = server.server_addr();
    let app_url = format!("http://{}", actual_addr);

    println!("====================================================");
    println!(" My Computer Keeper (Rust Core Engine + Desktop UI)");
    println!(" Native Desktop Application Mode: Active");
    println!(" Local RPC & UI Host listening on: {}", app_url);
    println!("====================================================");

    let last_activity = Arc::new(AtomicU64::new(current_timestamp_secs()));
    let shutdown_pending = Arc::new(AtomicBool::new(false));
    let has_connected = Arc::new(AtomicBool::new(false));
    let ui_dist = locate_ui_dist();

    let server_arc = Arc::new(server);
    let s_clone = Arc::clone(&server_arc);
    let last_clone = Arc::clone(&last_activity);
    let shut_clone = Arc::clone(&shutdown_pending);
    let conn_clone = Arc::clone(&has_connected);

    let server_thread = std::thread::spawn(move || {
        run_http_service(s_clone, ui_dist, shut_clone, conn_clone, last_clone);
    });

    if headless {
        println!("Headless / API mode running. Waiting for incoming requests...");
        let _ = server_thread.join();
    } else {
        // 啟動真正的 Windows 原生桌面視窗
        if let Err(err) = run_desktop_window(&app_url) {
            eprintln!("Failed to launch native desktop window: {}. Falling back to browser...", err);
            launch_browser_fallback(&app_url);

            // 備援瀏覽器模式下的生命週期守護
            let act = Arc::clone(&last_activity);
            let shut = Arc::clone(&shutdown_pending);
            let conn = Arc::clone(&has_connected);
            let start = Instant::now();

            loop {
                std::thread::sleep(Duration::from_millis(1000));
                let workflow_running = keeper_core::vhdx::compact_vhdx_status().state == keeper_core::vhdx::VhdxCompactState::Running;
                if shut.load(Ordering::SeqCst) && !workflow_running {
                    std::thread::sleep(Duration::from_millis(1500));
                    if shut.load(Ordering::SeqCst)
                        && keeper_core::vhdx::compact_vhdx_status().state != keeper_core::vhdx::VhdxCompactState::Running {
                        std::process::exit(0);
                    }
                }
                if conn.load(Ordering::SeqCst) && !workflow_running && start.elapsed() > Duration::from_secs(30) {
                    let now = current_timestamp_secs();
                    let last = act.load(Ordering::SeqCst);
                    if now.saturating_sub(last) > 90 {
                        std::process::exit(0);
                    }
                }
            }
        }
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::commands::dispatch_command;
    use super::resolve_static_resource;
    use serde_json::json;

    #[test]
    fn test_window_state_serialization() {
        let state = super::WindowState {
            x: 120,
            y: 80,
            width: 1440,
            height: 920,
            is_maximized: false,
        };
        let json = serde_json::to_string(&state).expect("serialize window state");
        let decoded: super::WindowState = serde_json::from_str(&json).expect("deserialize window state");
        assert_eq!(state, decoded);
    }

    #[test]
    fn test_embedded_ui_resolution() {
        // 驗證即使不指定外部 dist_dir，單獨執行檔也能從內嵌 EmbeddedUi 中正確解析 index.html
        let asset = resolve_static_resource("/", None);
        assert!(asset.is_some(), "Embedded index.html should be resolvable even without external ui/dist");
        let res = asset.unwrap();
        assert_eq!(res.content_type, "text/html; charset=utf-8");
        assert!(!res.payload.as_slice().is_empty());

        // 測試 SPA fallback
        let fallback_asset = resolve_static_resource("/treesize/explore", None);
        assert!(fallback_asset.is_some(), "SPA routes should fallback to index.html");
    }

    #[test]
    fn test_dispatch_vhdx_status_and_recovery_guards() {
        let response = dispatch_command("compact_vhdx_status", &json!({}));
        assert_eq!(response["ok"], true);
        let data = &response["data"];
        assert_eq!(data["state"], "idle");
        assert!(data["progress_percent"].is_null());
        assert_eq!(data["compact_completed"], false);
        assert_eq!(data["recovery_required"], false);
        assert_eq!(data["steps"].as_array().unwrap().len(), 6);
        assert_eq!(data["steps"][3]["id"], "compact");
        assert!(data["environment"].get("docker_exe").is_none());
        let recovery = dispatch_command("retry_vhdx_restore", &json!({}));
        assert_eq!(recovery["ok"], false);
        assert!(recovery["error"].as_str().is_some());
        let invalid = dispatch_command("compact_vhdx", &json!({ "path": "" }));
        assert_eq!(invalid["ok"], false);
        assert_eq!(dispatch_command("compact_vhdx_status", &json!({}))["data"]["state"], "idle");
    }

    #[test]
    fn test_dispatch_get_drives_and_capabilities() {
        let res = dispatch_command("get_drives", &json!({}));
        assert_eq!(res.get("ok").and_then(|v| v.as_bool()), Some(true));

        let net_res = dispatch_command(
            "inspect_capabilities",
            &json!({ "path": r"\\nas-box\media\photos" }),
        );
        assert_eq!(net_res.get("ok").and_then(|v| v.as_bool()), Some(true));
        let is_net = net_res
            .get("data")
            .and_then(|d| d.get("is_network_drive"))
            .and_then(|v| v.as_bool());
        assert_eq!(is_net, Some(true));

        let prev_res = dispatch_command(
            "preview_entry",
            &json!({ "path": "Cargo.toml", "force_network_load": false }),
        );
        assert_eq!(prev_res.get("ok").and_then(|v| v.as_bool()), Some(true));
        let kind = prev_res
            .get("data")
            .and_then(|d| d.get("preview_kind"))
            .and_then(|v| v.as_str());
        assert_eq!(kind, Some("text"));
    }

    #[test]
    fn test_dispatch_treesize_async_scan() {
        let start_res = dispatch_command(
            "start_treesize_scan",
            &json!({ "path": ".", "max_depth": 2 }),
        );
        assert_eq!(start_res.get("ok").and_then(|v| v.as_bool()), Some(true));
        let scan_id = start_res
            .get("data")
            .and_then(|d| d.get("scan_id"))
            .and_then(|v| v.as_str())
            .expect("scan_id string");
        assert!(!scan_id.is_empty());

        let prog_res = dispatch_command(
            "get_treesize_progress",
            &json!({ "scan_id": scan_id }),
        );
        assert_eq!(prog_res.get("ok").and_then(|v| v.as_bool()), Some(true));

        let cancel_res = dispatch_command(
            "cancel_treesize_scan",
            &json!({ "scan_id": scan_id }),
        );
        assert_eq!(cancel_res.get("ok").and_then(|v| v.as_bool()), Some(true));
    }
}
