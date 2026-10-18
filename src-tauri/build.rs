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

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set("FileDescription", "My Computer Keeper Desktop Application");
        res.set("ProductName", "My Computer Keeper");
        res.set("CompanyName", "Minus-Wu");
        res.set("LegalCopyright", "Copyright (C) 2026 Minus-Wu");
        res.set("OriginalFilename", "my-computer-keeper.exe");
        res.set_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and Windows 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}" />
    </application>
  </compatibility>
</assembly>
"#);
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile windows resource: {}", e);
        }
    }
}
