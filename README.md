# My Computer Keeper (全能電腦維運與檔案管家)

結合 **「檔案總管 + 工作管理員 (含 Port 獵人) + TreeSize + LockHunter + FreeFileSync + 電腦本體維運中心」** 的現代化桌面應用程式。以 **Rust (`keeper-core`)** 為系統核心引擎，搭配 **Tauri v2 / React + TypeScript + TailwindCSS** 桌面介面（方案 A），並內建 **網路磁碟機 (Network Drive / SMB / UNC) 智慧識別與有限功能降級保護機制**。

---

## ✨ 核心功能一覽

1. **📁 檔案總管 (`keeper-explorer`)**
   - 自動探索本機磁碟與已掛載的網路磁碟機（解析遠端 `\\Server\Share` UNC 路徑）。
   - **系統與常見資料夾知識庫**：智慧辨識 Windows 系統核心目錄（如 System32, SysWOW64, WinSxS, Program Files, AppData, Temp 等）與使用者/開發者常見目錄（Desktop, Downloads, .git, node_modules, target, .ssh 等），提供用途說明、安全警告指引與直接項目容量統計。
   - **全方位檔案預覽**：
     - **圖片**：PNG, JPG, JPEG, GIF, WebP, BMP, ICO, SVG 即時解碼與無段縮放檢視。
     - **純文字與程式原始碼**：Markdown（支援原始碼/排版切換）、CSV/TSV（支援原始碼/表格切換）、JSON、Rust、TypeScript、Python、Bash、SQL 等，支援行號顯示與一鍵複製。
     - **PDF 文件**：內嵌原生 PDF 閱讀檢視器，支援分頁與縮放。
     - **ZIP 壓縮檔**：免解壓縮即時檢視內部檔案清單、目錄結構與壓縮節省率。
     - **影音媒體**：MP3, WAV, FLAC, OGG 音訊與 MP4, WebM 影片即時播放。
     - **二進位與未知檔案**：經典 16 進位 Hex Dump 檢視與 Magic Bytes 檔案特徵碼自動分析。
   - **網路磁碟安全護欄**：遠端超過 15 MB 的圖片與大檔預設啟用按需載入保護；刪除遠端檔案時強制提示網路磁碟不支援本機資源回收筒。
2. **🍰 TreeSize 磁碟空間分析與 Treemap (`keeper-treesize`)**
   - 高速統計目錄空間、檔案類型分布（圖片、影片、程式碼、壓縮檔、文件）與 Top 20 巨型檔案。
   - **互動式 Treemap（矩形樹狀圖）**：支援點擊子資料夾直接下鑽（Drill-down）分析。
   - 針對網路磁碟機自動限制掃描深度與併發數，保護遠端 SMB 伺服器。
3. **📊 高階工作管理員 & Port 連接埠獵人 (`keeper-taskmgr`)**
   - 即時監控 CPU、實體記憶體 (RAM)、Swap 與活躍處理程序清單，支援一鍵終止行程。
   - **Port Hunter**：列出本機所有開啟的 TCP/UDP Port 與對應的 PID/Process，支援開發者快速搜尋並一鍵釋放被佔用的連接埠。
4. **🔓 LockHunter 檔案佔用解鎖器 (`keeper-lockhunter`)**
   - 整合 Windows **Restart Manager API (`rstrtmgr.dll`)** 與行程工作目錄分析，精準揪出鎖定檔案或資料夾的元兇行程。
   - 支援一鍵強制終止鎖定行程，或排程於下次 Windows 重開機時刪除（針對網路磁碟機自動標示 `LocalProcessOnly` 範圍並禁用重開機刪除）。
5. **🔄 FreeFileSync 智慧檔案同步器 (`keeper-sync`)**
   - 支援「修改時間 + 檔案大小」或「`BLAKE3` 加密級極速內容雜湊」雙模式比對。
   - 當網路磁碟機參與同步時，自動啟用 **2 秒 SMB 時間戳記容差**，解決 NAS (Linux/Samba) 與 Windows NTFS 時間精度差異。
   - 支援 **鏡像 (Mirror)**、**更新 (Update)**、**雙向互補 (Two-Way)** 同步與 **Dry-Run 預演模式**。
6. **🛠️ 電腦本體維運中心 (`keeper-ops`)**
   - **開發者與系統垃圾清理**：一鍵掃描 `target`, `node_modules`, `.venv`, `__pycache__`, `dist` 與系統 `Temp` 暫存區。
   - **重複檔案與重複圖片查殺**：以大小分組 + `BLAKE3` 雜湊找出重複檔案與圖片。
   - **系統 `PATH` 環境變數體檢 & 開機啟動項檢視 & 磁碟健康度摘要**。

---

## 🚀 快速啟動與驗證

### 1. 建置前端桌面介面 (`ui/`)
```powershell
cd ui
npm.cmd install
npm.cmd run build
```

### 2. 執行 Rust 單元測試與程式碼檢查
```powershell
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### 3. 啟動桌面應用程式
```powershell
cargo run -p my-computer-keeper
```
（啟動後將自動運行 Rust 核心引擎服務並開啟桌面視窗；開發模式下亦可同時於 `ui/` 執行 `npm.cmd run dev` 進行熱重載開發。）

---

## 📄 開發計畫書
完整架構選型與分階段規劃請參閱 [docs/DEVELOPMENT_PLAN.md](./docs/DEVELOPMENT_PLAN.md)。
