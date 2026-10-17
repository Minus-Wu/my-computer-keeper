# My Computer Keeper (全能電腦維運與檔案管家)

結合 **「檔案總管 (含圖片檢視) + 工作管理員 (含 Port 獵人) + TreeSize + LockHunter + FreeFileSync + 電腦本體維運中心」** 的現代化桌面應用程式。以 **Rust (`keeper-core`)** 為系統核心引擎，搭配 **Tauri v2 / React + TypeScript + TailwindCSS** 桌面介面（方案 A），並內建 **網路磁碟機 (Network Drive / SMB / UNC) 智慧識別與有限功能降級保護機制**。

---

## ✨ 核心功能一覽

1. **📁 檔案總管 & 圖片顯示檢視器 (`keeper-explorer`)**
   - 自動探索本機磁碟與已掛載的網路磁碟機（解析遠端 `\\Server\Share` UNC 路徑）。
   - 內建 Rust `image` 解碼引擎：支援 PNG, JPG, JPEG, GIF, WebP, BMP, ICO, SVG 即時圖片預覽、解析度與長寬比檢測、無段縮放檢視。
   - **網路磁碟安全護欄**：遠端超過 15 MB 的圖片預設啟用按需載入保護；刪除遠端檔案時強制提示網路磁碟不支援本機資源回收筒。
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
