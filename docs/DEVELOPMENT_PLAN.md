# My Computer Keeper (全能電腦維運與檔案管家) 開發計畫書

本專案以 **Rust** 為核心引擎，結合 **Tauri v2 + React + TypeScript + TailwindCSS**（方案 A）打造現代化桌面應用程式，整合 **「檔案總管 + 工作管理員 + TreeSize + LockHunter + FreeFileSync」** 五大工具與 **電腦本體維運中心（System Ops）**，並針對 **網路磁碟機（Network Drive / SMB / UNC）** 提供智慧識別與「有限功能安全降級模式」。

---

## 一、 技術架構選型 (方案 A：Tauri v2 + Rust Core Workspace)

- **核心引擎層 (`crates/keeper-core`)**：100% Rust 實作，負責高速檔案系統走訪、圖片解碼與縮圖生成、Windows 磁碟與網路磁碟識別 (`GetDriveTypeW` / `WNetGetConnectionW`)、Windows Restart Manager 檔案佔用查詢、系統 Process 與 Port 監控、`BLAKE3` 差異比對與同步、以及系統維運清理。
- **桌面橋接層 (`src-tauri` / 雙模伺服器)**：提供 Tauri IPC Commands 與本機開發 API 橋接，讓前端能以毫秒級延遲呼叫 Rust 核心功能。
- **前端呈現層 (`ui/`)**：React 19 + TypeScript + Vite + TailwindCSS + Lucide Icons，支援圖片瀑布流/大圖檢視（含 EXIF/解析度資訊）、TreeSize 互動式矩形樹狀圖（Treemap）、即時資源監控圖表與 FreeFileSync 並排差異比較視圖。

---

## 二、 網路磁碟機 (Network Drive / SMB / UNC) 有限功能設計規範

系統會自動檢測所有磁碟代號與路徑是否屬於 **網路磁碟機 (`DriveKind::Network` 或 `\\server\share` UNC 路徑)**，並自動套用 `VolumeCapabilities` 降級保護策略：

| 功能模組 | 本機磁碟 (`Fixed` / `Removable`) | 網路磁碟機 (`Network` / `UNC` — 有限功能模式) |
| :--- | :--- | :--- |
| **磁碟資訊與識別** | 顯示本機磁區、檔案系統與容量 | 自動解析遠端 **UNC 路徑 (`\\NAS\Share`)**，顯示 `🌐 網路磁碟 (有限功能)` 警示標籤 |
| **檔案總管** | 系統與常見資料夾知識庫、無限制自動縮圖、純文字/程式碼、PDF、ZIP 壓縮檔、影音與 Hex 預覽、移至資源回收筒 | **預覽保護**：超過 `15 MB` 的遠端圖片與大檔預設暫停自動下載，需點擊「強制載入」以防網路卡死；**刪除保護**：網路磁碟不支援 Windows 資源回收筒，刪除前強制提示將直接永久刪除 |
| **TreeSize 空間分析** | 支援全速多執行緒掃描（最高 16+ 執行緒）與無上限深度 | **受控 I/O 掃描**：限制併發數（預設 4 執行緒）、限制預設最大掃描深度（6 層）與最大項目數（25,000 項），避免打爆 SMB 伺服器連線 |
| **FreeFileSync 同步** | 支援「時間+大小」與全速 `BLAKE3` 雜湊比對 | **SMB 時間戳容差**：預設採用「時間+大小（含 **2 秒時間戳容差**）」解決 Linux/NAS 與 Windows 時間精度差異；若啟用 `BLAKE3` 雜湊則提示全檔網路讀取警告 |
| **LockHunter 解鎖** | 支援查詢所有本機鎖定行程、強制關閉與「下次重開機刪除」 | **僅限本機行程 (`LocalProcessOnly`)**：僅能查詢「當前電腦」開啟該遠端檔案的 Process，無法得知遠端 NAS 上其他電腦的鎖定；且停用「下次重開機刪除」 |
| **電腦本體維運** | 完整支援重複檔案/相似圖片、開發垃圾清理、S.M.A.R.T. | S.M.A.R.T. 硬體健康度標示為「不支援網路磁碟」；重複檔案掃描預設僅比對檔案大小與首尾特徵，避免大量網路讀取 |

---

## 三、 六大核心模組功能規格

1. **📁 檔案總管 (File Explorer & Multi-Format Inspector)**
   - 本機與網路磁碟機自動探索（含容量條、遠端 UNC 路徑顯示、有限功能徽章）。
   - **系統與常見資料夾說明庫**：智慧識別 Windows 核心目錄（System32, SysWOW64, WinSxS, ProgramData, AppData 等）與常用開發/使用者目錄，提供詳細用途、安全警示與直屬項目統計。
   - **全方位預覽支援**：包含圖片 (PNG/JPG/WebP/SVG)、純文字與程式碼 (Markdown/JSON/CSV/Rust/TS 等，含行號與複製)、PDF 內嵌檢視器、ZIP 壓縮檔免解壓目錄清單、影音播放 (MP3/MP4) 與二進位 Hex Dump 檔案特徵碼分析。
2. **📊 高階工作管理員 + Port 獵人 (Task Manager & Port Hunter)**
   - 即時監控 CPU 使用率、記憶體、Swap 與磁碟狀態。
   - 行程管理清單（PID、名稱、CPU%、記憶體、執行路徑），支援搜尋與一鍵終止行程（Kill Process）。
   - **Port 佔用監控**：列出本機所有開啟的 TCP/UDP 連接埠、狀態與對應的 PID/Process，支援開發者快速釋放被佔用的連接埠。
3. **🍰 TreeSize 磁碟空間分析器 (Disk Space Analyzer)**
   - 高速統計目錄結構大小、檔案數、子目錄數。
   - **互動式 Treemap（矩形樹狀圖）** 視覺化呈現各子目錄與檔案佔比。
   - 檔案類型分布統計（圖片、影片、程式碼、壓縮檔、文件、其他）與 Top 20 巨型檔案排行。
4. **🔓 LockHunter 檔案佔用解鎖器 (File Lock Inspector)**
   - 整合 Windows **Restart Manager API (`rstrtmgr.dll`)**，輸入或從檔案總管選擇任何檔案/資料夾，即可揪出鎖定該路徑的 Process 清單（PID、名稱、應用程式類型、是否可重啟）。
   - 支援一鍵強制終止鎖定行程、或排程於下次重開機時刪除（僅限本機磁碟）。
5. **🔄 FreeFileSync 智慧檔案同步器 (Smart Directory Sync)**
   - 支援左右雙目錄比對（`TimeAndSize` 含網路磁碟 2 秒容差、或 `Blake3Hash` 加密級極速雜湊）。
   - 視覺化差異分類（僅左側、僅右側、左側較新、右側較新、內容不同、完全相同）。
   - 支援 **鏡像 (Mirror)**、**更新 (Update)**、**雙向增補 (Two-Way)** 同步模式與 Dry-Run 預演。
6. **🛠️ 電腦本體維運中心 (System Maintenance & Ops)**
   - **開發者與系統垃圾清理**：一鍵掃描指定工作區下的 `target` (Rust)、`node_modules` (Node.js)、`.venv` / `__pycache__` (Python)、`dist` 等大型建置快取，以及系統 `Temp` 暫存區，支援單項或批次清除。
   - **重複檔案與重複圖片搜尋**：利用大小分組 + 首尾快速雜湊 + `BLAKE3` 找出重複檔案與重複圖片。
   - **開機啟動項檢視**與 **系統 `PATH` 環境變數體檢**（自動標示失效路徑或指向網路磁碟機的高延遲路徑）。

---

## 四、 分階段實施進度

- [x] **Phase 1**：架構選型確認（方案 A：Tauri v2 + Rust Workspace + React/TS）與網路磁碟機有限功能規格制定。
- [x] **Phase 2**：建立 `keeper-core` 核心引擎（含 Windows 磁碟/網路磁碟識別、五大核心模組與維運模組實作及完整單元測試）。
- [x] **Phase 3**：建立 Tauri v2 後端與 React + TailwindCSS 現代化整合桌面介面（含圖片預覽、Treemap、Diff 視圖、Port 監控與網路磁碟安全護欄）。
- [x] **Phase 4**：執行 Linter / Type Check / Rust 單元測試與建置驗證，並推送至 GitHub。
