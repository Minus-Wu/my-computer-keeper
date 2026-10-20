use crate::drive::{inspect_path_capabilities, normalize_display_path, VolumeCapabilities};
use crate::{KeeperError, KeeperResult};
use base64::Engine;
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Directory,
    Image,
    Video,
    Audio,
    Code,
    Archive,
    Document,
    Executable,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size_bytes: u64,
    pub modified_unix: u64,
    pub extension: String,
    pub category: FileCategory,
    pub is_image: bool,
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub capabilities: VolumeCapabilities,
    pub total_dirs: usize,
    pub total_files: usize,
    pub total_images: usize,
    pub total_size_bytes: u64,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePreviewResult {
    pub path: String,
    pub name: String,
    pub file_size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub format_name: String,
    pub color_type: String,
    pub aspect_ratio: String,
    pub data_url: Option<String>,
    pub deferred_for_network: bool,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownFolderInfo {
    pub is_system_or_known: bool,
    pub category_label: String,
    pub badge_color: String,
    pub summary: String,
    pub description: String,
    pub safety_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryPreviewResult {
    pub name: String,
    pub path: String,
    pub is_system_or_known: bool,
    pub category_label: String,
    pub badge_color: String,
    pub summary: String,
    pub description: String,
    pub safety_warning: Option<String>,
    pub direct_subdirs_count: usize,
    pub direct_files_count: usize,
    pub direct_size_bytes: u64,
    pub has_permission_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPreviewResult {
    pub content: String,
    pub total_lines: usize,
    pub preview_lines: usize,
    pub is_truncated: bool,
    pub syntax: String,
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipEntryItem {
    pub name: String,
    pub is_dir: bool,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipPreviewResult {
    pub total_entries: usize,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_uncompressed_bytes: u64,
    pub total_compressed_bytes: u64,
    pub overall_ratio: f64,
    pub entries: Vec<ZipEntryItem>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfPreviewResult {
    pub data_url: Option<String>,
    pub file_size_bytes: u64,
    pub version: String,
    pub notice: Option<String>,
    pub deferred_for_network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPreviewResult {
    pub media_type: String,
    pub format: String,
    pub data_url: Option<String>,
    pub file_size_bytes: u64,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexRow {
    pub offset: String,
    pub hex: String,
    pub ascii: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexPreviewResult {
    pub file_size_bytes: u64,
    pub rows: Vec<HexRow>,
    pub magic_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralItemPreview {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub extension: String,
    pub file_size_bytes: u64,
    pub modified_unix: u64,
    pub preview_kind: String,
    pub dir_preview: Option<DirectoryPreviewResult>,
    pub image_preview: Option<ImagePreviewResult>,
    pub text_preview: Option<TextPreviewResult>,
    pub pdf_preview: Option<PdfPreviewResult>,
    pub zip_preview: Option<ZipPreviewResult>,
    pub media_preview: Option<MediaPreviewResult>,
    pub hex_preview: Option<HexPreviewResult>,
    pub notice: Option<String>,
}


/// 根據副檔名判斷檔案分類與是否為可預覽圖片
pub fn classify_extension(ext: &str, is_dir: bool) -> (FileCategory, bool) {
    if is_dir {
        return (FileCategory::Directory, false);
    }
    match ext.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" => {
            (FileCategory::Image, true)
        }
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" => (FileCategory::Video, false),
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" => (FileCategory::Audio, false),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "c" | "cpp" | "h" | "json" | "toml"
        | "yaml" | "yml" | "html" | "css" | "sql" | "sh" | "ps1" => (FileCategory::Code, false),
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zst" => {
            (FileCategory::Archive, false)
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv" => {
            (FileCategory::Document, false)
        }
        "exe" | "msi" | "bat" | "cmd" | "dll" => (FileCategory::Executable, false),
        _ => (FileCategory::Other, false),
    }
}

/// 讀取指定目錄內容，並附上磁碟（含網路磁碟有限功能）權限矩陣
pub fn list_directory(path_str: &str) -> KeeperResult<DirectoryListing> {
    let target = Path::new(path_str);
    if !target.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let canonical = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let clean_path = normalize_display_path(&canonical);
    let capabilities = inspect_path_capabilities(&clean_path);

    let parent_path = clean_path
        .parent()
        .map(|p| normalize_display_path(p).to_string_lossy().to_string())
        .filter(|p| !p.is_empty() && p != &clean_path.to_string_lossy());

    let read_dir = fs::read_dir(&clean_path)?;
    let mut entries = Vec::new();
    let mut total_dirs = 0usize;
    let mut total_files = 0usize;
    let mut total_images = 0usize;
    let mut total_size_bytes = 0u64;

    for item in read_dir.flatten() {
        let entry_path = item.path();
        let file_name = item.file_name().to_string_lossy().to_string();
        let meta = match item.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_dir = meta.is_dir();
        let is_symlink = meta.file_type().is_symlink();
        let size_bytes = if is_dir { 0 } else { meta.len() };
        let modified_unix = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let extension = entry_path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        let (category, is_image) = classify_extension(&extension, is_dir);
        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            total_size_bytes = total_size_bytes.saturating_add(size_bytes);
            if is_image {
                total_images += 1;
            }
        }

        entries.push(FileEntry {
            name: file_name,
            path: normalize_display_path(&entry_path)
                .to_string_lossy()
                .to_string(),
            is_dir,
            is_symlink,
            size_bytes,
            modified_unix,
            extension,
            category,
            is_image,
            readonly: meta.permissions().readonly(),
        });
    }

    // 排序：資料夾優先，接著依名稱不分大小寫排序
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase()),
    });

    Ok(DirectoryListing {
        current_path: clean_path.to_string_lossy().to_string(),
        parent_path,
        capabilities,
        total_dirs,
        total_files,
        total_images,
        total_size_bytes,
        entries,
    })
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// 載入並解碼圖片預覽（支援 PNG/JPG/GIF/WebP/BMP/ICO/SVG），同時處理網路磁碟機大檔保護
pub fn preview_image(
    path_str: &str,
    max_dimension: u32,
    force_network_load: bool,
) -> KeeperResult<ImagePreviewResult> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let meta = fs::metadata(path)?;
    let file_size = meta.len();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path_str.to_string());
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let capabilities = inspect_path_capabilities(path);
    if capabilities.is_network_drive
        && file_size > capabilities.max_auto_preview_bytes
        && !force_network_load
    {
        return Ok(ImagePreviewResult {
            path: path_str.to_string(),
            name,
            file_size_bytes: file_size,
            width: 0,
            height: 0,
            format_name: ext.to_ascii_uppercase(),
            color_type: "Deferred (Network Safe Mode)".to_string(),
            aspect_ratio: "-".to_string(),
            data_url: None,
            deferred_for_network: true,
            notice: Some(format!(
                "此圖片位於網路磁碟機且大小 ({:.1} MB) 超過 15 MB 自動預覽上限，為避免網路阻塞已暫停自動載入。可點擊「強制載入原圖」檢視。",
                file_size as f64 / (1024.0 * 1024.0)
            )),
        });
    }

    // 處理 SVG 向量圖
    if ext == "svg" {
        let raw_bytes = fs::read(path)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&raw_bytes);
        return Ok(ImagePreviewResult {
            path: path_str.to_string(),
            name,
            file_size_bytes: file_size,
            width: 800,
            height: 600,
            format_name: "SVG".to_string(),
            color_type: "Vector XML".to_string(),
            aspect_ratio: "Scalable".to_string(),
            data_url: Some(format!("data:image/svg+xml;base64,{}", encoded)),
            deferred_for_network: false,
            notice: None,
        });
    }

    let raw_bytes = fs::read(path)?;
    let decoded = image::load_from_memory(&raw_bytes)
        .map_err(|e| KeeperError::ImageError(e.to_string()))?;

    let (width, height) = decoded.dimensions();
    let color_type = format!("{:?}", decoded.color());
    let divisor = gcd(width, height);
    let aspect_ratio = format!("{}:{}", width / divisor, height / divisor);

    let limit = max_dimension.clamp(128, 2048);
    let resized = if width > limit || height > limit {
        decoded.thumbnail(limit, limit)
    } else {
        decoded
    };

    let mut png_buf = Cursor::new(Vec::new());
    resized
        .write_to(&mut png_buf, image::ImageFormat::Png)
        .map_err(|e| KeeperError::ImageError(e.to_string()))?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(png_buf.into_inner());

    Ok(ImagePreviewResult {
        path: path_str.to_string(),
        name,
        file_size_bytes: file_size,
        width,
        height,
        format_name: ext.to_ascii_uppercase(),
        color_type,
        aspect_ratio,
        data_url: Some(format!("data:image/png;base64,{}", encoded)),
        deferred_for_network: false,
        notice: if capabilities.is_network_drive {
            Some("已透過網路磁碟機安全通道載入圖片預覽。".to_string())
        } else {
            None
        },
    })
}

/// 刪除檔案或資料夾（含網路磁碟機不支援資源回收筒的安全檢查）
pub fn delete_entry(
    path_str: &str,
    use_recycle_bin: bool,
    confirm_permanent_on_network: bool,
) -> KeeperResult<String> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let caps = inspect_path_capabilities(path);
    if caps.is_network_drive && use_recycle_bin && !confirm_permanent_on_network {
        return Err(KeeperError::NetworkDriveRestriction(
            "網路磁碟機 (SMB/UNC) 不支援移至 Windows 本機資源回收筒。若要刪除此遠端項目，請啟用「確認在網路磁碟上永久刪除」。".to_string(),
        ));
    }

    if use_recycle_bin && !caps.is_network_drive {
        trash::delete(path).map_err(|e| KeeperError::SystemError(e.to_string()))?;
        Ok(format!("已將「{}」安全移至資源回收筒。", path_str))
    } else {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(format!("已永久刪除「{}」。", path_str))
    }
}

/// 判斷資料夾是否為系統或常見已知目錄，並提供相對應的繁體中文詳細說明與安全指引
pub fn inspect_known_folder(dir_name: &str, full_path: &str) -> KnownFolderInfo {
    let name_lower = dir_name.trim().to_ascii_lowercase();
    let path_lower = full_path.replace('/', "\\").to_ascii_lowercase();

    if name_lower == "system32" || path_lower.ends_with(r"\windows\system32") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Windows 核心系統".to_string(),
            badge_color: "rose".to_string(),
            summary: "Windows 64 位元核心系統檔案與驅動程式目錄".to_string(),
            description: "存放作業系統核心組件、系統 DLL 函式庫、硬體裝置驅動程式、HAL、以及內建系統工具（如 cmd.exe, taskmgr.exe 等）。此目錄為 Windows 最重要的運作中樞。".to_string(),
            safety_warning: Some("嚴禁任意刪除、修改或覆蓋此資料夾內的任何檔案，否則將導致作業系統藍屏崩潰或無法開機。".to_string()),
        }
    } else if name_lower == "syswow64" || path_lower.ends_with(r"\windows\syswow64") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Windows 系統相容庫".to_string(),
            badge_color: "rose".to_string(),
            summary: "Windows-on-Windows 64-bit 32 位元應用程式相容庫".to_string(),
            description: "在 64 位元 Windows 系統中，專為提供 32 位元傳統應用程式執行環境所設計的相容 DLL 程式庫與子系統目錄。".to_string(),
            safety_warning: Some("系統核心目錄，請勿更動或刪除，否則相依的 32 位元應用程式與系統組件將無法啟動。".to_string()),
        }
    } else if name_lower == "winsxs" || path_lower.ends_with(r"\windows\winsxs") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Windows 元件存放區".to_string(),
            badge_color: "amber".to_string(),
            summary: "Windows Side-by-Side 原生元件市集與更新備份存放庫".to_string(),
            description: "儲存系統元件的多個歷史版本以避免 DLL 地獄 (DLL Hell)，並存放 Windows Update 更新備份與系統自我修復檔案。其體積會隨系統更新而增長。".to_string(),
            safety_warning: Some("切勿直接從檔案總管手動刪除此目錄內的檔案！若需瘦身，請使用 Windows DISM 工具或磁碟清理 (dism /online /cleanup-image /startcomponentcleanup)。".to_string()),
        }
    } else if name_lower == "windows" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "作業系統根目錄".to_string(),
            badge_color: "rose".to_string(),
            summary: "Microsoft Windows 作業系統主要安裝根目錄".to_string(),
            description: "包含作業系統核心架構、桌面環境、字型、系統排程工作、開機載入器與所有系統設定檔，是整台電腦軟體運作的基石。".to_string(),
            safety_warning: Some("作業系統核心根目錄，切勿嘗試移動、重新命名或刪除。".to_string()),
        }
    } else if name_lower == "program files" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "軟體安裝目錄".to_string(),
            badge_color: "blue".to_string(),
            summary: "64 位元應用程式的標準預設安裝目錄".to_string(),
            description: "儲存已安裝的 64 位元桌面應用程式本體、執行檔 (.exe)、動態連結庫 (.dll)、外掛模組與資產檔案。".to_string(),
            safety_warning: Some("請透過 Windows「設定 > 應用程式與功能」正常解除安裝軟體，切勿直接以檔案總管暴力刪除整個資料夾，以免殘留無效登錄檔。".to_string()),
        }
    } else if name_lower == "program files (x86)" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "軟體安裝目錄 (32-bit)".to_string(),
            badge_color: "blue".to_string(),
            summary: "32 位元傳統應用程式在 64 位元 Windows 上的安裝目錄".to_string(),
            description: "專供 32 位元架構的應用程式與舊版軟體安裝存放。Windows 會透過 WoW64 子系統將此目錄與 64 位元隔離。".to_string(),
            safety_warning: Some("若需移除程式，請務必透過官方解除安裝程式，避免直接刪除資料夾。".to_string()),
        }
    } else if name_lower == "programdata" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "全域應用程式資料".to_string(),
            badge_color: "indigo".to_string(),
            summary: "所有使用者共享的應用程式資料、設定與快取目錄".to_string(),
            description: "預設為隱藏資料夾，存放不分使用者帳號、全電腦所有使用者通用的軟體組態設定、本機授權證書、共用快取資料庫以及防毒軟體特徵碼等。".to_string(),
            safety_warning: Some("部分軟體在此儲存重要設定與授權資料，刪除可能導致軟體重置或授權失效。".to_string()),
        }
    } else if name_lower == "users" || name_lower == "使用者" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "本機使用者總目錄".to_string(),
            badge_color: "cyan".to_string(),
            summary: "電腦上所有使用者帳戶個人資料夾的匯集根目錄".to_string(),
            description: "包含本機每一位已註冊使用者的個人家目錄，底下包含各自專屬的桌面、下載、文件、圖片、影片以及 AppData 設定檔，具備 NTFS 使用者存取權限隔離。".to_string(),
            safety_warning: Some("管理員可檢視其他使用者資料夾，但請避免直接變更其存取控制清單 (ACL) 造成使用者登入異常。".to_string()),
        }
    } else if name_lower == "appdata" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "應用程式數據主目錄".to_string(),
            badge_color: "purple".to_string(),
            summary: "當前登入使用者的應用程式資料主目錄 (%AppData%)".to_string(),
            description: "Windows 預設隱藏資料夾，細分為 Local（本機專屬）、LocalLow（沙盒低權限）與 Roaming（隨帳戶漫遊）三個子目錄，保存所有軟體的個人化設定、瀏覽器記錄與工作進度。".to_string(),
            safety_warning: Some("直接刪除此目錄會導致軟體設定全部遺失、登入狀態登出或軟體無法正常開啟。".to_string()),
        }
    } else if name_lower == "local" && path_lower.contains("appdata") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "本機應用程式數據".to_string(),
            badge_color: "purple".to_string(),
            summary: "儲存不隨網域漫遊的使用者本機專屬快取與軟體設定 (AppData\\Local)".to_string(),
            description: "存放各應用程式在當前電腦上的本機快取、大型資料庫、擴充插件、暫存檔 (Temp) 以及許多單一使用者安裝軟體（如 VS Code、Chrome、Spotify 等）。".to_string(),
            safety_warning: Some("若軟體異常可嘗試清理特定子資料夾，但請勿刪除整個 Local 目錄。底下的 Temp 資料夾則可安全定期清理。".to_string()),
        }
    } else if name_lower == "roaming" && path_lower.contains("appdata") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "漫遊應用程式設定".to_string(),
            badge_color: "purple".to_string(),
            summary: "隨使用者設定檔漫遊的個人化組態 (AppData\\Roaming)".to_string(),
            description: "存放使用者的客製化設定檔，如文字編輯器偏好設定、鍵盤快速鍵、瀏覽器書籤、FTP 連線設定等，在 Windows 網域漫遊環境下會同步至伺服器。".to_string(),
            safety_warning: Some("刪除軟體對應的 Roaming 目錄會重置該軟體至初始預設狀態。".to_string()),
        }
    } else if name_lower == "locallow" && path_lower.contains("appdata") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "低權限沙箱資料".to_string(),
            badge_color: "purple".to_string(),
            summary: "受保護模式或沙箱環境應用程式專用的儲存空間 (AppData\\LocalLow)".to_string(),
            description: "專供受限完整性層級 (Low Integrity Level) 的行程（如受保護模式的網頁瀏覽器控制項、Java 外掛等）寫入資料，以防惡意代碼竄改本機系統。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "temp" || name_lower == "tmp" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "暫存檔案目錄".to_string(),
            badge_color: "emerald".to_string(),
            summary: "系統與應用程式執行過程中產生的暫存快取目錄 (%TEMP%)".to_string(),
            description: "用來存放軟體安裝解壓縮中繼檔、瀏覽器暫存檔、自動儲存快照以及各程式執行時的暫存資料。".to_string(),
            safety_warning: Some("💡 此目錄可定期安全清理！清理已被釋放的暫存檔案能釋出寶貴的磁碟空間；若有檔案提示被鎖定，表示該程式正在運行，跳過即可。".to_string()),
        }
    } else if name_lower == "prefetch" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "系統預先載入快取".to_string(),
            badge_color: "amber".to_string(),
            summary: "Windows 預先載入最佳化資料庫 (C:\\Windows\\Prefetch)".to_string(),
            description: "Windows 系統追蹤應用程式與系統開機時載入的 DLL 與檔案清單，藉由預測讀取優化應用程式的開啟反應時間。".to_string(),
            safety_warning: Some("若手動清空，Windows 會在後續使用過程中自動重新建立；通常無需頻繁手動清理。".to_string()),
        }
    } else if name_lower == "fonts" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "系統字型庫".to_string(),
            badge_color: "sky".to_string(),
            summary: "Windows 系統字型存放庫 (C:\\Windows\\Fonts)".to_string(),
            description: "儲存系統內建與使用者安裝的所有字型檔案（TTF, OTF, TTC 等）。在此目錄可預覽字型外觀與字元樣式。".to_string(),
            safety_warning: Some("請勿刪除 Windows 預設的系統字型（如 Segoe UI, 微軟正黑體 等），以免系統介面文字顯示異常。".to_string()),
        }
    } else if name_lower == "$recycle.bin" || path_lower.contains("$recycle.bin") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "資源回收筒中繼站".to_string(),
            badge_color: "amber".to_string(),
            summary: "Windows 資源回收筒底層實體存放目錄".to_string(),
            description: "每個 NTFS 磁碟分割區上的隱藏系統目錄，以使用者的 Windows SID 為子目錄，保存使用者移至資源回收筒但尚未「清空回收筒」的檔案與刪除中繼紀錄。".to_string(),
            safety_warning: Some("建議透過桌面「資源回收筒」圖示執行清空，不建議直接對底層目錄執行硬刪除。".to_string()),
        }
    } else if name_lower == "system volume information" || path_lower.contains("system volume information") {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "磁碟區保護資訊".to_string(),
            badge_color: "rose".to_string(),
            summary: "Windows 磁碟區陰影複製與還原點專用目錄".to_string(),
            description: "每個磁碟分割區根目錄下的系統級受保護目錄，用以儲存「系統還原點 (System Restore Points)」、磁碟區陰影複製 (VSS Snapshots) 及 Windows Search 索引檔案。".to_string(),
            safety_warning: Some("此目錄受到 Windows 系統最高安全權限保護（僅 SYSTEM 可寫），切勿強行取得擁有權修改，以防系統還原機制損壞。".to_string()),
        }
    } else if name_lower == "recovery" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Windows 復原環境".to_string(),
            badge_color: "rose".to_string(),
            summary: "Windows 復原環境映像檔 (WinRE) 存放目錄".to_string(),
            description: "儲存 Winre.wim 開機復原映像與自動診斷工具，當 Windows 出現開機異常時會自動載入此環境進行系統自我修復。".to_string(),
            safety_warning: Some("系統重要保護目錄，請勿刪除。".to_string()),
        }
    } else if name_lower == "desktop" || name_lower == "桌面" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "個人桌面目錄".to_string(),
            badge_color: "cyan".to_string(),
            summary: "使用者桌面圖示與檔案存放目錄".to_string(),
            description: "直接映射至使用者螢幕桌面所展示的捷徑、檔案與資料夾。存放於此的內容會直接顯示於電腦桌面空間。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "downloads" || name_lower == "下載" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "個人下載目錄".to_string(),
            badge_color: "cyan".to_string(),
            summary: "瀏覽器與通訊軟體的預設下載存放位置".to_string(),
            description: "存放從網路下載的安裝檔、文件、圖片與各類壓縮包。長時間未整理容易佔用大量磁碟空間。".to_string(),
            safety_warning: Some("💡 建議定期檢視並將不需使用的歷史安裝包 (.exe, .msi, .zip) 刪除以釋放空間。".to_string()),
        }
    } else if name_lower == "documents" || name_lower == "我的文件" || name_lower == "文件" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "個人文件目錄".to_string(),
            badge_color: "cyan".to_string(),
            summary: "使用者個人文件、辦公檔案與軟體存檔目錄".to_string(),
            description: "存放 Office 文件、PDF 報告、程式設計專案檔以及許多桌面軟體的自訂範本與預設儲存檔。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "pictures" || name_lower == "我的圖片" || name_lower == "圖片" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "個人圖片目錄".to_string(),
            badge_color: "pink".to_string(),
            summary: "相片、圖片素材、相簿與螢幕截圖存放目錄".to_string(),
            description: "Windows 相片應用程式、繪圖軟體以及系統截圖 (Win + PrtScn) 的預設存檔資料庫。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "music" || name_lower == "我的音樂" || name_lower == "音樂" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "個人音樂目錄".to_string(),
            badge_color: "teal".to_string(),
            summary: "音訊檔案、音樂收藏與播放清單存放目錄".to_string(),
            description: "存放 MP3, FLAC, WAV 等音訊檔案，提供給 Windows 媒體播放器或音樂軟體建立歌曲索引資料庫。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "videos" || name_lower == "我的影片" || name_lower == "影片" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "個人影片目錄".to_string(),
            badge_color: "teal".to_string(),
            summary: "錄影檔案、視訊剪輯素材與電影存放目錄".to_string(),
            description: "存放 MP4, MKV 等影片檔，同時也是 Windows 遊戲列錄影 (Xbox Game Bar Captures) 的預設儲存位置。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "saved games" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "遊戲存檔目錄".to_string(),
            badge_color: "violet".to_string(),
            summary: "PC 電腦遊戲進度存檔與個人設定檔存放位置".to_string(),
            description: "符合 Windows 遊戲標準規範的多數 PC 單機遊戲會將遊戲存檔、角色紀錄與個人設定儲存於此目錄。".to_string(),
            safety_warning: Some("刪除此資料夾內的子目錄可能會遺失對應遊戲的闖關進度存檔。".to_string()),
        }
    } else if name_lower == "favorites" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "書籤與捷徑".to_string(),
            badge_color: "amber".to_string(),
            summary: "網際網路瀏覽器我的最愛與 URL 連結捷徑".to_string(),
            description: "Windows 傳統我的最愛目錄，包含使用者保存的網址捷徑 (.url) 與書籤分類資料夾。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "contacts" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "通訊錄".to_string(),
            badge_color: "blue".to_string(),
            summary: "Windows 本機聯絡人名片資料存放目錄".to_string(),
            description: "儲存 .contact 格式的名片資料，提供給 Windows 內建連絡人與電子郵件軟體使用。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "links" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "快速捷徑".to_string(),
            badge_color: "blue".to_string(),
            summary: "檔案總管快速存取導覽窗格的連結捷徑".to_string(),
            description: "儲存指向常用資料夾與網路位置的捷徑 (.lnk)，用以加速檔案總管中的切換跳轉。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == ".git" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Git 版本控制倉儲".to_string(),
            badge_color: "emerald".to_string(),
            summary: "Git 分散式版本控制系統的核心中繼資料庫".to_string(),
            description: "包含 Git 倉儲的完整物件資料庫 (objects)、所有提交 Commit 歷史紀錄、分支指標 (refs/heads)、標籤 (tags) 以及設定檔 (config)。".to_string(),
            safety_warning: Some("⚠️ 切勿手動刪除或修改 .git 資料夾！刪除將導致專案失去所有未推送的版本控制歷史記錄與分支資訊。".to_string()),
        }
    } else if name_lower == ".github" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "GitHub 工作流程設定".to_string(),
            badge_color: "indigo".to_string(),
            summary: "GitHub Actions CI/CD 自動化流程與協作範本目錄".to_string(),
            description: "存放 GitHub Actions 自動化建置工作流程 (.github/workflows/*.yml)、Issue 與 Pull Request 範本、以及貢獻指南設定。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == ".vscode" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "VS Code 專案設定".to_string(),
            badge_color: "blue".to_string(),
            summary: "Visual Studio Code 工作區專屬組態目錄".to_string(),
            description: "儲存該專案在 VS Code 中的專屬設定 (settings.json)、除錯啟動組態 (launch.json)、任務 (tasks.json) 以及推薦延伸模組 (extensions.json)。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == ".idea" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "JetBrains 專案設定".to_string(),
            badge_color: "indigo".to_string(),
            summary: "JetBrains 系列 IDE（IntelliJ, WebStorm 等）專案設定目錄".to_string(),
            description: "存放 IDE 專案模組結構、程式碼樣式規則、編譯執行組態與版本控制整合參數。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "node_modules" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Node.js 相依套件庫".to_string(),
            badge_color: "emerald".to_string(),
            summary: "JavaScript / TypeScript 專案依賴套件安裝目錄".to_string(),
            description: "由 npm, yarn 或 pnpm 自動下載並維護的第三方函式庫。此目錄通常包含龐大數量的檔案與深層嵌套資料夾。".to_string(),
            safety_warning: Some("📦 此資料夾可安全刪除！若需清理空間或重新安裝依賴，刪除後在專案根目錄執行 `npm install` 即可自動重新下載還原。".to_string()),
        }
    } else if name_lower == "target" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "編譯產物輸出目錄".to_string(),
            badge_color: "amber".to_string(),
            summary: "Rust Cargo 或 Java Maven 專案的二進位建置產出目錄".to_string(),
            description: "包含編譯產生的可執行檔、動態連結檔、中間依賴快取 (.rmeta) 與偵錯符號資訊。建置大型專案時體積可達數 GB。".to_string(),
            safety_warning: Some("📦 此資料夾可安全清除！在 Rust 專案中執行 `cargo clean` 即可一鍵清空釋放空間，後續 `cargo build` 會重新編譯。".to_string()),
        }
    } else if name_lower == "dist" || name_lower == "build" || name_lower == "out" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "發布建置成果目錄".to_string(),
            badge_color: "cyan".to_string(),
            summary: "前端或軟體工程經打包編譯後的正式上線發布目錄".to_string(),
            description: "儲存經壓縮、混淆、最佳化後的 HTML/CSS/JS、二進位發布包或 Docker 映像檔中繼產物，準備部署至正式環境伺服器。".to_string(),
            safety_warning: Some("可透過專案的建置指令（如 `npm run build`）重新產生。".to_string()),
        }
    } else if name_lower == "vendor" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "第三方依賴原始碼".to_string(),
            badge_color: "indigo".to_string(),
            summary: "第三方相依函式庫原始碼本地封裝目錄 (Go / PHP 等)".to_string(),
            description: "將專案所需的外部套件原始碼直接隨倉儲保存，以實現離線建置與版本鎖定。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "venv" || name_lower == ".venv" || name_lower == "env" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Python 虛擬環境".to_string(),
            badge_color: "emerald".to_string(),
            summary: "Python 獨立隔離運作環境與套件庫目錄".to_string(),
            description: "包含獨立的 Python 解譯器副本、專屬 pip 套件安裝路徑 (site-packages) 與環境啟動腳本，確保專案套件版本與系統 Python 隔離。".to_string(),
            safety_warning: Some("若有 requirements.txt，可隨時刪除此目錄並透過 `python -m venv venv` 重新建立。".to_string()),
        }
    } else if name_lower == "__pycache__" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Python 字節碼快取".to_string(),
            badge_color: "amber".to_string(),
            summary: "Python 解譯器編譯的 .pyc 字節碼快取目錄".to_string(),
            description: "存放 Python 載入模組後編譯產生的字節碼快取檔案，以加速後續執行程式時的匯入效率。".to_string(),
            safety_warning: Some("💡 可安全刪除！Python 再次執行時會自動重新編譯產生。".to_string()),
        }
    } else if name_lower == ".cargo" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "Rust Cargo 組態與快取".to_string(),
            badge_color: "amber".to_string(),
            summary: "Rust 套件管理器 Cargo 的全域設定檔與下載套件快取".to_string(),
            description: "儲存 crates.io 索引資料庫、Git 依賴快取、預設編譯目標與本機工具組態設定。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == ".ssh" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "SSH 金鑰憑證目錄".to_string(),
            badge_color: "rose".to_string(),
            summary: "SSH 身份認證金鑰與伺服器主機指紋保存庫 (~/.ssh)".to_string(),
            description: "存放 SSH 私鑰 (id_rsa, id_ed25519)、公鑰 (.pub)、known_hosts（已認證遠端主機指紋）以及 ssh config 連線設定檔。".to_string(),
            safety_warning: Some("🔒 極高機敏安全資料！私鑰絕對不可對外公開洩漏或隨意分享，請妥善做好備份保護。".to_string()),
        }
    } else if name_lower == "cache" || name_lower == ".cache" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "應用程式快取".to_string(),
            badge_color: "purple".to_string(),
            summary: "軟體或系統運行產生的本機快取目錄".to_string(),
            description: "存放快取影像、網頁離線資料、字型快取或索引資料庫，以減少重複運算與網路請求。".to_string(),
            safety_warning: Some("通常可安全清除釋放磁碟空間，軟體後續會自動重新建立快取。".to_string()),
        }
    } else if name_lower == "logs" || name_lower == "log" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "系統與軟體日誌".to_string(),
            badge_color: "slate".to_string(),
            summary: "程式運作記錄檔與錯誤追蹤日誌存放目錄".to_string(),
            description: "儲存軟體、資料庫或系統服務在運行過程中所輸出的存取紀錄、除錯診斷與例外崩潰追蹤檔案。".to_string(),
            safety_warning: Some("可定期封存或刪除過期的歷史記錄檔 (.log) 以釋放磁碟空間。".to_string()),
        }
    } else if name_lower == "bin" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "二進位執行檔目錄".to_string(),
            badge_color: "cyan".to_string(),
            summary: "可執行二進位程式、指令檔或腳本存放目錄".to_string(),
            description: "存放已編譯的二進位程式檔、Shell 腳本、批次檔 (.bat) 或命令列工具，通常包含在系統 PATH 環境變數中。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "src" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "原始程式碼核心目錄".to_string(),
            badge_color: "blue".to_string(),
            summary: "軟體開發專案的原始程式碼 (Source Code) 核心存放區".to_string(),
            description: "存放專案核心邏輯、程式模組、資料型別與功能實現程式碼，是整個軟體專案的核心智慧財產。".to_string(),
            safety_warning: Some("專案最核心資產，請務必納入 Git 版本控制並定期遠端備份。".to_string()),
        }
    } else if name_lower == "public" || name_lower == "static" || name_lower == "assets" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "公開靜態資源".to_string(),
            badge_color: "sky".to_string(),
            summary: "網站或軟體之公開靜態素材存放目錄".to_string(),
            description: "存放不需經由編譯處理的原生靜態資源，例如網站圖標 (favicon.ico)、圖片資產、品牌 Logo、音效素材與全域樣式表。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "docs" || name_lower == "doc" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "技術文件與說明手冊".to_string(),
            badge_color: "teal".to_string(),
            summary: "專案技術規格書、架構說明與開發者指引目錄".to_string(),
            description: "存放 Markdown 格式的專案手冊、API 規格說明、系統架構圖 (Architecture Diagrams) 以及版本更新日誌 (Changelog)。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "tests" || name_lower == "test" || name_lower == "spec" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "自動化測試套件".to_string(),
            badge_color: "emerald".to_string(),
            summary: "單元測試、整合測試與端對端 (E2E) 測試程式庫".to_string(),
            description: "存放自動化測試案例、測試假資料 (Mocks/Fixtures) 以及回歸測試腳本，確保程式碼修改不產生非預期錯誤。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "drivers" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "硬體驅動程式".to_string(),
            badge_color: "rose".to_string(),
            summary: "硬體設備驅動程式安裝套件或備份目錄".to_string(),
            description: "包含主機板、顯示卡、網路卡或音效卡等硬體裝置所需的 .inf, .sys 驅動安裝包與設定組態。".to_string(),
            safety_warning: None,
        }
    } else if name_lower == "etc" {
        KnownFolderInfo {
            is_system_or_known: true,
            category_label: "系統組態設定".to_string(),
            badge_color: "slate".to_string(),
            summary: "系統與服務軟體之靜態設定檔目錄".to_string(),
            description: "源自 Unix 慣例的系統組態目錄，存放網路 hosts 對應檔、系統全域設定檔與服務初始化組態。".to_string(),
            safety_warning: None,
        }
    } else {
        KnownFolderInfo {
            is_system_or_known: false,
            category_label: "一般資料夾".to_string(),
            badge_color: "slate".to_string(),
            summary: "一般檔案系統目錄".to_string(),
            description: "用於組織與分類檔案之標準本機或網路資料夾。可透過檔案總管自由瀏覽、移動或管理其中的檔案。".to_string(),
            safety_warning: None,
        }
    }
}

/// 格式化前段二進位 bytes 為 16 進位 Hex Dump，並偵測魔術位元組
pub fn generate_hex_preview(bytes: &[u8], total_size: u64) -> HexPreviewResult {
    let mut rows = Vec::new();
    let max_bytes = bytes.len().min(512);

    for chunk_start in (0..max_bytes).step_by(16) {
        let chunk_end = (chunk_start + 16).min(max_bytes);
        let chunk = &bytes[chunk_start..chunk_end];

        let offset = format!("{:08X}", chunk_start);

        let mut hex_parts = Vec::new();
        for (i, b) in chunk.iter().enumerate() {
            if i == 8 {
                hex_parts.push(format!(" {:02X}", b));
            } else {
                hex_parts.push(format!("{:02X}", b));
            }
        }
        let padding = if chunk.len() < 16 {
            let missing = 16 - chunk.len();
            "   ".repeat(missing) + if chunk.len() < 8 { " " } else { "" }
        } else {
            String::new()
        };
        let hex_str = format!("{}{}", hex_parts.join(" "), padding);

        let ascii_str: String = chunk
            .iter()
            .map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' })
            .collect();

        rows.push(HexRow {
            offset,
            hex: hex_str,
            ascii: ascii_str,
        });
    }

    let magic_description = if bytes.starts_with(&[0x4D, 0x5A]) {
        Some("Windows PE 可執行檔 / 動態程式庫 (MZ / EXE / DLL / SYS)".to_string())
    } else if bytes.starts_with(&[0x7F, 0x45, 0x4C, 0x46]) {
        Some("Linux ELF 可執行檔 / 共享物件庫 (.so)".to_string())
    } else if bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        Some("PKZip 壓縮檔 / Office Open XML (ZIP / DOCX / XLSX / JAR)".to_string())
    } else if bytes.starts_with(b"%PDF") {
        Some("Adobe PDF 文件格式 (%PDF)".to_string())
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some("PNG 點陣圖影像格式".to_string())
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("JPEG 影像格式".to_string())
    } else if bytes.starts_with(b"GIF8") {
        Some("GIF 影像格式".to_string())
    } else if bytes.starts_with(b"BM") {
        Some("Windows BMP 點陣圖".to_string())
    } else if bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        Some("7-Zip 高壓縮比壓縮檔 (.7z)".to_string())
    } else if bytes.starts_with(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07]) {
        Some("RAR 壓縮檔案格式".to_string())
    } else if bytes.starts_with(&[0x1F, 0x8B]) {
        Some("GZIP 壓縮檔案格式 (.gz)".to_string())
    } else if bytes.starts_with(b"BZh") {
        Some("BZIP2 壓縮檔案格式 (.bz2)".to_string())
    } else if bytes.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        Some("XZ 壓縮檔案格式 (.xz)".to_string())
    } else if bytes.starts_with(b"SQLite format 3\0") {
        Some("SQLite 3 關聯式資料庫檔案".to_string())
    } else {
        None
    };

    HexPreviewResult {
        file_size_bytes: total_size,
        rows,
        magic_description,
    }
}

/// 通用檔案與資料夾全方位預覽函數
pub fn preview_general_entry(
    path_str: &str,
    force_network_load: bool,
) -> KeeperResult<GeneralItemPreview> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let meta = fs::metadata(path)?;
    let is_dir = meta.is_dir();
    let file_size_bytes = if is_dir { 0 } else { meta.len() };
    let modified_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path_str.to_string());

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let capabilities = inspect_path_capabilities(path);

    // 1. 若為資料夾：提供已知/系統目錄知識庫說明與直接子項目快速統計
    if is_dir {
        let known = inspect_known_folder(&name, path_str);
        let mut subdirs = 0usize;
        let mut files = 0usize;
        let mut dir_size = 0u64;
        let mut has_perm_error = false;

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(m) = entry.metadata() {
                    if m.is_dir() {
                        subdirs += 1;
                    } else {
                        files += 1;
                        dir_size = dir_size.saturating_add(m.len());
                    }
                }
            }
        } else {
            has_perm_error = true;
        }

        let dir_preview = DirectoryPreviewResult {
            name: name.clone(),
            path: path_str.to_string(),
            is_system_or_known: known.is_system_or_known,
            category_label: known.category_label,
            badge_color: known.badge_color,
            summary: known.summary,
            description: known.description,
            safety_warning: known.safety_warning,
            direct_subdirs_count: subdirs,
            direct_files_count: files,
            direct_size_bytes: dir_size,
            has_permission_error: has_perm_error,
        };

        return Ok(GeneralItemPreview {
            path: path_str.to_string(),
            name,
            is_dir: true,
            extension: String::new(),
            file_size_bytes: dir_size,
            modified_unix,
            preview_kind: "directory".to_string(),
            dir_preview: Some(dir_preview),
            image_preview: None,
            text_preview: None,
            pdf_preview: None,
            zip_preview: None,
            media_preview: None,
            hex_preview: None,
            notice: if capabilities.is_network_drive {
                Some("此目錄位於網路磁碟機 (SMB/UNC)。".to_string())
            } else {
                None
            },
        });
    }

    // 2. 圖片格式：PNG, JPG, JPEG, GIF, WEBP, BMP, ICO, SVG
    let is_image_ext = matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg"
    );

    if is_image_ext {
        let img_res = preview_image(path_str, 960, force_network_load)?;
        return Ok(GeneralItemPreview {
            path: path_str.to_string(),
            name,
            is_dir: false,
            extension: ext,
            file_size_bytes,
            modified_unix,
            preview_kind: "image".to_string(),
            dir_preview: None,
            image_preview: Some(img_res),
            text_preview: None,
            pdf_preview: None,
            zip_preview: None,
            media_preview: None,
            hex_preview: None,
            notice: None,
        });
    }

    // 3. ZIP 壓縮檔預覽 (ZIP, JAR, WAR, APK)
    let is_zip_ext = matches!(ext.as_str(), "zip" | "jar" | "war" | "apk");
    if is_zip_ext {
        if capabilities.is_network_drive && file_size_bytes > 50 * 1024 * 1024 && !force_network_load {
            return Ok(GeneralItemPreview {
                path: path_str.to_string(),
                name,
                is_dir: false,
                extension: ext,
                file_size_bytes,
                modified_unix,
                preview_kind: "zip".to_string(),
                dir_preview: None,
                image_preview: None,
                text_preview: None,
                pdf_preview: None,
                zip_preview: None,
                media_preview: None,
                hex_preview: None,
                notice: Some("此 ZIP 壓縮檔位於網路磁碟機且大於 50 MB，為節省頻寬已暫緩讀取目錄清單。可點選「強制載入」檢視。".to_string()),
            });
        }

        if let Ok(file) = fs::File::open(path) {
            if let Ok(mut archive) = zip::ZipArchive::new(file) {
                let total_entries = archive.len();
                let mut total_files = 0usize;
                let mut total_dirs = 0usize;
                let mut total_uncompressed = 0u64;
                let mut total_compressed = 0u64;
                let mut entry_items = Vec::new();

                let limit = total_entries.min(300);
                for i in 0..limit {
                    if let Ok(entry) = archive.by_index(i) {
                        let is_entry_dir = entry.is_dir();
                        let unc = entry.size();
                        let comp = entry.compressed_size();
                        let ratio = if unc > 0 {
                            (1.0 - (comp as f64 / unc as f64)) * 100.0
                        } else {
                            0.0
                        };

                        if is_entry_dir {
                            total_dirs += 1;
                        } else {
                            total_files += 1;
                            total_uncompressed = total_uncompressed.saturating_add(unc);
                            total_compressed = total_compressed.saturating_add(comp);
                        }

                        entry_items.push(ZipEntryItem {
                            name: entry.name().to_string(),
                            is_dir: is_entry_dir,
                            uncompressed_size: unc,
                            compressed_size: comp,
                            compression_ratio: ratio.max(0.0),
                        });
                    }
                }

                let overall_ratio = if total_uncompressed > 0 {
                    (1.0 - (total_compressed as f64 / total_uncompressed as f64)) * 100.0
                } else {
                    0.0
                };

                let comment_str = String::from_utf8_lossy(archive.comment()).to_string();
                let comment = if comment_str.trim().is_empty() {
                    None
                } else {
                    Some(comment_str)
                };

                return Ok(GeneralItemPreview {
                    path: path_str.to_string(),
                    name,
                    is_dir: false,
                    extension: ext,
                    file_size_bytes,
                    modified_unix,
                    preview_kind: "zip".to_string(),
                    dir_preview: None,
                    image_preview: None,
                    text_preview: None,
                    pdf_preview: None,
                    zip_preview: Some(ZipPreviewResult {
                        total_entries,
                        total_files,
                        total_dirs,
                        total_uncompressed_bytes: total_uncompressed,
                        total_compressed_bytes: total_compressed,
                        overall_ratio: overall_ratio.max(0.0),
                        entries: entry_items,
                        comment,
                    }),
                    media_preview: None,
                    hex_preview: None,
                    notice: if total_entries > 300 {
                        Some(format!("此壓縮檔包含 {} 個項目，已為您顯示前 300 個項目清單。", total_entries))
                    } else {
                        None
                    },
                });
            }
        }
    }

    // 4. PDF 文件預覽
    if ext == "pdf" {
        let pdf_limit = if capabilities.is_network_drive { 5 * 1024 * 1024 } else { 25 * 1024 * 1024 };
        if file_size_bytes > pdf_limit && !force_network_load {
            return Ok(GeneralItemPreview {
                path: path_str.to_string(),
                name,
                is_dir: false,
                extension: ext,
                file_size_bytes,
                modified_unix,
                preview_kind: "pdf".to_string(),
                dir_preview: None,
                image_preview: None,
                text_preview: None,
                pdf_preview: Some(PdfPreviewResult {
                    data_url: None,
                    file_size_bytes,
                    version: "PDF".to_string(),
                    notice: Some(format!(
                        "此 PDF 檔案大小 ({:.1} MB) 超過自動載入上限，為確保操作流暢已暫緩載入完整文件。可點擊下方按鈕強制載入以在內嵌檢視器中閱覽。",
                        file_size_bytes as f64 / (1024.0 * 1024.0)
                    )),
                    deferred_for_network: true,
                }),
                zip_preview: None,
                media_preview: None,
                hex_preview: None,
                notice: None,
            });
        }

        let raw_bytes = fs::read(path)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&raw_bytes);
        let header_slice = if raw_bytes.len() > 16 { &raw_bytes[..16] } else { &raw_bytes };
        let version_str = String::from_utf8_lossy(header_slice)
            .lines()
            .next()
            .unwrap_or("PDF")
            .trim_start_matches('%')
            .to_string();

        return Ok(GeneralItemPreview {
            path: path_str.to_string(),
            name,
            is_dir: false,
            extension: ext,
            file_size_bytes,
            modified_unix,
            preview_kind: "pdf".to_string(),
            dir_preview: None,
            image_preview: None,
            text_preview: None,
            pdf_preview: Some(PdfPreviewResult {
                data_url: Some(format!("data:application/pdf;base64,{}", encoded)),
                file_size_bytes,
                version: version_str,
                notice: None,
                deferred_for_network: false,
            }),
            zip_preview: None,
            media_preview: None,
            hex_preview: None,
            notice: None,
        });
    }

    // 5. 音訊與影片 (Audio / Video)
    let is_audio = matches!(ext.as_str(), "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac");
    let is_video = matches!(ext.as_str(), "mp4" | "webm");
    if is_audio || is_video {
        let media_limit = 25 * 1024 * 1024;
        if file_size_bytes <= media_limit {
            if let Ok(bytes) = fs::read(path) {
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let mime = if is_audio {
                    match ext.as_str() {
                        "mp3" => "audio/mpeg",
                        "wav" => "audio/wav",
                        "ogg" => "audio/ogg",
                        "flac" => "audio/flac",
                        "aac" => "audio/aac",
                        _ => "audio/mp4",
                    }
                } else {
                    match ext.as_str() {
                        "webm" => "video/webm",
                        _ => "video/mp4",
                    }
                };

                return Ok(GeneralItemPreview {
                    path: path_str.to_string(),
                    name,
                    is_dir: false,
                    extension: ext.clone(),
                    file_size_bytes,
                    modified_unix,
                    preview_kind: "media".to_string(),
                    dir_preview: None,
                    image_preview: None,
                    text_preview: None,
                    pdf_preview: None,
                    zip_preview: None,
                    media_preview: Some(MediaPreviewResult {
                        media_type: if is_audio { "audio".to_string() } else { "video".to_string() },
                        format: ext.to_ascii_uppercase(),
                        data_url: Some(format!("data:{};base64,{}", mime, encoded)),
                        file_size_bytes,
                        notice: None,
                    }),
                    hex_preview: None,
                    notice: None,
                });
            }
        }
    }

    // 6. 純文字與原始碼預覽
    let is_text_ext = matches!(
        ext.as_str(),
        "txt"
            | "log"
            | "ini"
            | "cfg"
            | "conf"
            | "env"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "xml"
            | "csv"
            | "tsv"
            | "md"
            | "markdown"
            | "rs"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "cs"
            | "html"
            | "css"
            | "scss"
            | "sql"
            | "sh"
            | "bash"
            | "ps1"
            | "bat"
            | "cmd"
            | "gitignore"
            | "dockerfile"
            | "makefile"
            | "r"
            | "lua"
            | "perl"
            | "php"
    );

    let max_read_bytes = 128 * 1024;
    let mut file = fs::File::open(path)?;
    let mut buffer = Vec::new();
    let mut chunk = vec![0u8; max_read_bytes];
    let bytes_read = {
        use std::io::Read;
        file.read(&mut chunk)?
    };
    buffer.extend_from_slice(&chunk[..bytes_read]);

    let is_truncated = file_size_bytes > bytes_read as u64;

    let sample = if buffer.len() > 1024 { &buffer[..1024] } else { &buffer };
    let has_null_bytes = sample.contains(&0);


    if (is_text_ext || !has_null_bytes) && !buffer.is_empty() {
        let content_string = String::from_utf8_lossy(&buffer).to_string();
        let total_lines = content_string.lines().count();
        let syntax = match ext.as_str() {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "py" => "python",
            "json" => "json",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "md" | "markdown" => "markdown",
            "csv" => "csv",
            "tsv" => "tsv",
            "html" => "html",
            "css" | "scss" => "css",
            "sql" => "sql",
            "sh" | "bash" => "bash",
            "ps1" => "powershell",
            "bat" | "cmd" => "batch",
            "xml" => "xml",
            "c" | "cpp" | "h" | "hpp" => "cpp",
            "go" => "go",
            "java" => "java",
            "cs" => "csharp",
            _ => "text",
        };

        return Ok(GeneralItemPreview {
            path: path_str.to_string(),
            name,
            is_dir: false,
            extension: ext,
            file_size_bytes,
            modified_unix,
            preview_kind: "text".to_string(),
            dir_preview: None,
            image_preview: None,
            text_preview: Some(TextPreviewResult {
                content: content_string,
                total_lines,
                preview_lines: total_lines,
                is_truncated,
                syntax: syntax.to_string(),
                encoding: "UTF-8".to_string(),
            }),
            pdf_preview: None,
            zip_preview: None,
            media_preview: None,
            hex_preview: None,
            notice: if is_truncated {
                Some(format!("檔案較大 ({:.1} MB)，已為您快速預覽前 128 KB 內容。", file_size_bytes as f64 / (1024.0 * 1024.0)))
            } else {
                None
            },
        });
    }

    // 7. 二進位 / 其他檔案以 16 進位 Hex Dump 檢視
    let hex_res = generate_hex_preview(&buffer, file_size_bytes);
    Ok(GeneralItemPreview {
        path: path_str.to_string(),
        name,
        is_dir: false,
        extension: ext,
        file_size_bytes,
        modified_unix,
        preview_kind: "hex".to_string(),
        dir_preview: None,
        image_preview: None,
        text_preview: None,
        pdf_preview: None,
        zip_preview: None,
        media_preview: None,
        hex_preview: Some(hex_res),
        notice: Some("此檔案為二進位或未格式化檔案，已為您提供 16 進位位元組 (Hex Dump) 與標頭特徵分析。".to_string()),
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    #[test]
    fn test_list_directory_and_image_preview() {
        let dir = tempdir().expect("create temp dir");
        let img_path = dir.path().join("sample.png");

        // 建立一張 32x16 測試圖片
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(32, 16, |x, y| Rgba([x as u8 * 8, y as u8 * 16, 180, 255]));
        img.save(&img_path).expect("save png");

        let txt_path = dir.path().join("notes.txt");
        fs::write(&txt_path, "hello world").expect("write txt");

        let listing =
            list_directory(&dir.path().to_string_lossy()).expect("list directory succeeds");
        assert_eq!(listing.total_files, 2);
        assert_eq!(listing.total_images, 1);

        let preview =
            preview_image(&img_path.to_string_lossy(), 512, false).expect("preview image succeeds");
        assert_eq!(preview.width, 32);
        assert_eq!(preview.height, 16);
        assert_eq!(preview.aspect_ratio, "2:1");
        assert!(preview
            .data_url
            .as_deref()
            .unwrap_or("")
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_known_folder_inspection() {
        let sys32 = inspect_known_folder("System32", r"C:\Windows\System32");
        assert!(sys32.is_system_or_known);
        assert_eq!(sys32.category_label, "Windows 核心系統");
        assert!(sys32.safety_warning.is_some());

        let downloads = inspect_known_folder("Downloads", r"C:\Users\User\Downloads");
        assert!(downloads.is_system_or_known);
        assert_eq!(downloads.category_label, "個人下載目錄");

        let git = inspect_known_folder(".git", r"D:\Project\.git");
        assert!(git.is_system_or_known);
        assert_eq!(git.category_label, "Git 版本控制倉儲");

        let custom = inspect_known_folder("MyRandomFolder", r"D:\MyRandomFolder");
        assert!(!custom.is_system_or_known);
        assert_eq!(custom.category_label, "一般資料夾");
    }

    #[test]
    fn test_preview_general_entry_text_and_dir() {
        let dir = tempdir().expect("create temp dir");
        let txt_file = dir.path().join("readme.md");
        fs::write(&txt_file, "# Project Title\nThis is a test readme file.\nLine 3").expect("write file");

        // 測試純文字 (Markdown) 預覽
        let prev = preview_general_entry(&txt_file.to_string_lossy(), false).expect("preview text succeeds");
        assert_eq!(prev.preview_kind, "text");
        let text_data = prev.text_preview.expect("has text data");
        assert_eq!(text_data.syntax, "markdown");
        assert_eq!(text_data.total_lines, 3);
        assert!(text_data.content.contains("# Project Title"));

        // 測試資料夾預覽
        let dir_prev = preview_general_entry(&dir.path().to_string_lossy(), false).expect("preview dir succeeds");
        assert_eq!(dir_prev.preview_kind, "directory");
        let dir_data = dir_prev.dir_preview.expect("has dir data");
        assert_eq!(dir_data.direct_files_count, 1);
        assert_eq!(dir_data.direct_subdirs_count, 0);
    }
}

