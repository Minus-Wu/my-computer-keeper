use crate::drive::{inspect_path_capabilities, normalize_display_path, VolumeCapabilities};
use crate::{KeeperError, KeeperResult};
use base64::Engine;
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}
