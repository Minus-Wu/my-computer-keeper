use std::fs;
use std::path::Path;

fn main() {
    let ui_dist = Path::new("../ui/dist");
    if !ui_dist.exists() {
        let _ = fs::create_dir_all(ui_dist);
    }
    let index_html = ui_dist.join("index.html");
    if !index_html.exists() {
        let fallback_html = r#"<!DOCTYPE html>
<html lang="zh-TW">
<head>
    <meta charset="UTF-8">
    <title>My Computer Keeper</title>
</head>
<body>
    <div id="root" style="font-family: sans-serif; padding: 2rem; text-align: center;">
        <h1>My Computer Keeper</h1>
        <p>尚未偵測到編譯完成的前端介面資源。請於 <code>ui/</code> 目錄執行 <code>npm run build</code> 重新建置前端。</p>
    </div>
</body>
</html>"#;
        let _ = fs::write(index_html, fallback_html);
    }
    println!("cargo:rerun-if-changed=../ui/dist");
}
