use anyhow::{Result, Context as AnyhowContext};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct ContextManager {
    root_dir: PathBuf,
    ignore_patterns: Vec<Regex>,
    max_file_size_kb: usize,
    max_context_size_kb: usize,
}

impl ContextManager {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self> {
        let root_dir = fs::canonicalize(root_dir)?;
        
        // Default ignore patterns
        let ignore_patterns = vec![
            Regex::new(r"\.git/")?,
            Regex::new(r"\.gitignore")?,
            Regex::new(r"node_modules/")?,
            Regex::new(r"target/")?,
            Regex::new(r"\.DS_Store")?,
            Regex::new(r"\.vscode/")?,
            Regex::new(r"\.idea/")?,
            Regex::new(r"\.(png|jpe?g|gif|svg|woff|woff2|ttf|eot|mp4|mp3|avi|mov|webm|pdf|zip|tar|gz|rar)$")?,
        ];
        
        Ok(Self {
            root_dir,
            ignore_patterns,
            max_file_size_kb: 100, // 100KB max file size
            max_context_size_kb: 8000, // 8MB max context size
        })
    }
    
    pub fn get_context(&self) -> Result<String> {
        let mut context = String::new();
        let mut total_size = 0;
        
        // Check if .gitignore exists and add its patterns
        let gitignore_path = self.root_dir.join(".gitignore");
        let mut gitignore_patterns = Vec::new();
        
        if gitignore_path.exists() {
            let gitignore_content = fs::read_to_string(&gitignore_path)?;
            for line in gitignore_content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    // Convert gitignore pattern to regex
                    // This is a simplified conversion and might not handle all gitignore syntax
                    let pattern = line
                        .replace(".", "\\.")
                        .replace("*", ".*")
                        .replace("?", ".");
                    
                    gitignore_patterns.push(Regex::new(&format!("^{}$", pattern))?);
                }
            }
        }
        
        // Collect files recursively
        for entry in WalkDir::new(&self.root_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let rel_path = path.strip_prefix(&self.root_dir).with_context(|| {
                format!("Failed to strip prefix from path: {:?}", path)
            })?;
            
            // Check if file should be ignored
            let rel_path_str = rel_path.to_string_lossy();
            if self.should_ignore(&rel_path_str, &gitignore_patterns) {
                continue;
            }
            
            // Check file size
            let metadata = fs::metadata(path)?;
            let file_size_kb = metadata.len() as usize / 1024;
            
            if file_size_kb > self.max_file_size_kb {
                continue;
            }
            
            // Skip binary files
            if crate::utils::is_binary_file(path)? {
                continue;
            }
            
            // Add file to context
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {:?}", path))?;
            
            let file_entry = format!("--- {}\n{}\n", rel_path_str, content);
            
            // Check if adding this file would exceed max context size
            let file_entry_size_kb = file_entry.len() / 1024;
            if total_size + file_entry_size_kb > self.max_context_size_kb {
                context.push_str(&format!("Note: Context truncated due to size limits\n"));
                break;
            }
            
            context.push_str(&file_entry);
            total_size += file_entry_size_kb;
        }
        
        Ok(context)
    }
    
    fn should_ignore(&self, rel_path: &str, gitignore_patterns: &[Regex]) -> bool {
        // Check built-in ignore patterns
        for pattern in &self.ignore_patterns {
            if pattern.is_match(rel_path) {
                return true;
            }
        }
        
        // Check gitignore patterns
        for pattern in gitignore_patterns {
            if pattern.is_match(rel_path) {
                return true;
            }
        }
        
        false
    }
}