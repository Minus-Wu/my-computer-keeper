mod commands;

use commands::dispatch_command;
use rust_embed::RustEmbed;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tiny_http::{Header, Method, Response, Server};

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

fn launch_desktop_app_window(url: &str) {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless" || a == "--api-only");
    let bind_addr = std::env::var("KEEPER_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:14280".to_string());

    let server = Server::http(&bind_addr).expect("Failed to bind My Computer Keeper RPC Host");
    let app_url = format!("http://{}", bind_addr);
    println!("====================================================");
    println!(" My Computer Keeper (Rust Core Engine + Desktop UI)");
    println!(" Standalone Single Executable Mode: Ready");
    println!(" Local RPC & UI Host listening on: {}", app_url);
    println!("====================================================");

    if !headless {
        launch_desktop_app_window(&app_url);
    }

    let ui_dist = locate_ui_dist();

    for mut request in server.incoming_requests() {
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

#[cfg(test)]
mod tests {
    use super::commands::dispatch_command;
    use super::resolve_static_resource;
    use serde_json::json;

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
}
