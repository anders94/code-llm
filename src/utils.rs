use anyhow::{Result, Context as AnyhowContext};
use std::fs;
use std::path::Path;

pub fn ensure_directory_exists<P: AsRef<Path>>(dir: P) -> Result<()> {
    let dir = dir.as_ref();
    if !dir.exists() {
        fs::create_dir_all(dir).with_context(|| {
            format!("Failed to create directory: {:?}", dir)
        })?;
    }
    Ok(())
}

pub fn is_binary_file<P: AsRef<Path>>(path: P) -> Result<bool> {
    let path = path.as_ref();
    
    // Check the file extension first
    if let Some(extension) = path.extension() {
        let ext = extension.to_string_lossy().to_lowercase();
        let binary_extensions = [
            "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg",
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            "zip", "tar", "gz", "rar", "7z",
            "exe", "dll", "so", "dylib",
            "mp3", "mp4", "avi", "mov", "webm",
            "woff", "woff2", "ttf", "eot",
        ];
        
        if binary_extensions.contains(&ext.as_str()) {
            return Ok(true);
        }
    }
    
    // Check file content for null bytes, which is a common way to detect binary files
    let content = fs::read(path).with_context(|| {
        format!("Failed to read file: {:?}", path)
    })?;
    
    // Check the first 8KB for null bytes
    let check_size = std::cmp::min(8192, content.len());
    for i in 0..check_size {
        if content[i] == 0 {
            return Ok(true);
        }
    }
    
    Ok(false)
}