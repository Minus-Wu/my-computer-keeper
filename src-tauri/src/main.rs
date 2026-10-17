mod commands;

use commands::dispatch_command;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tiny_http::{Header, Method, Response, Server};

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

fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "json" => "application/json; charset=utf-8",
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
        let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
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

        // 提供前端靜態檔案服務
        if let Some(ref dist_dir) = ui_dist {
            let clean_rel = url.trim_start_matches('/').split('?').next().unwrap_or("");
            let file_path = if clean_rel.is_empty() {
                dist_dir.join("index.html")
            } else {
                let candidate = dist_dir.join(clean_rel);
                if candidate.is_file() {
                    candidate
                } else {
                    dist_dir.join("index.html")
                }
            };

            if let Ok(bytes) = fs::read(&file_path) {
                let ctype = content_type_for_path(&file_path);
                let mut resp = Response::from_data(bytes);
                resp.add_header(
                    Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap(),
                );
                let _ = request.respond(resp);
                continue;
            }
        }

        let fallback = Response::from_string(
            "My Computer Keeper Rust RPC Server is running. Run `npm run dev` in `ui/` or build `ui/dist`.",
        );
        let _ = request.respond(fallback);
    }
}

#[cfg(test)]
mod tests {
    use super::commands::dispatch_command;
    use serde_json::json;

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

