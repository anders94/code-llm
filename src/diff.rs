use anyhow::{Result, anyhow, Context as AnyhowContext};
use regex::Regex;
use std::fs;
use std::path::PathBuf;
use similar::{ChangeTag, TextDiff};
use thiserror::Error;

use crate::utils::ensure_directory_exists;

#[derive(Error, Debug)]
pub enum DiffError {
    #[error("Invalid diff format: {0}")]
    InvalidFormat(String),
    
    #[error("File not found: {0}")]
    FileNotFound(String),
}

pub trait DiffAction {
    fn apply(&self) -> Result<()>;
    fn display_diff(&self) -> String;
}

#[derive(Debug)]
pub struct FileDiff {
    file_path: PathBuf,
    old_content: String,
    new_content: String,
    is_new_file: bool,
}

impl DiffAction for FileDiff {
    fn apply(&self) -> Result<()> {
        if self.is_new_file {
            // Create directories if they don't exist
            if let Some(parent) = self.file_path.parent() {
                ensure_directory_exists(parent)?;
            }
        } else if !self.file_path.exists() {
            return Err(anyhow!(DiffError::FileNotFound(
                self.file_path.to_string_lossy().to_string()
            )));
        }
        
        fs::write(&self.file_path, &self.new_content)
            .with_context(|| format!("Failed to write to file: {:?}", self.file_path))?;
        
        Ok(())
    }
    
    fn display_diff(&self) -> String {
        let path_str = self.file_path.to_string_lossy();
        
        if self.is_new_file {
            format!("New file: {}\n{}", path_str, self.new_content)
        } else {
            let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
            
            let mut diff_output = format!("File: {}\n", path_str);
            
            for op in diff.ops() {
                for change in diff.iter_changes(op) {
                    let (sign, value) = match change.tag() {
                        ChangeTag::Delete => ("-", change.value()),
                        ChangeTag::Insert => ("+", change.value()),
                        ChangeTag::Equal => continue,
                    };
                    diff_output.push_str(&format!("{}{}", sign, value));
                }
            }
            
            diff_output
        }
    }
}

pub struct DiffGenerator {
    diff_regex: Regex,
}

impl DiffGenerator {
    pub fn new() -> Self {
        let diff_regex = Regex::new(r"```diff\s*\n(.*?)```").unwrap();
        Self { diff_regex }
    }
    
    pub fn extract_diffs(&self, text: &str) -> Vec<FileDiff> {
        let mut diffs = Vec::new();
        
        for captures in self.diff_regex.captures_iter(text) {
            if let Some(diff_text) = captures.get(1) {
                if let Ok(diff) = self.parse_diff(diff_text.as_str()) {
                    diffs.push(diff);
                }
            }
        }
        
        diffs
    }
    
    fn parse_diff(&self, diff_text: &str) -> Result<FileDiff> {
        // Extract file path from the first line
        let lines: Vec<&str> = diff_text.lines().collect();
        
        if lines.is_empty() {
            return Err(anyhow!(DiffError::InvalidFormat(
                "Diff is empty".to_string()
            )));
        }
        
        let file_path = PathBuf::from(lines[0].trim());
        let is_new_file = !file_path.exists();
        
        let mut old_content = String::new();
        let mut new_content = String::new();
        
        if is_new_file {
            // For new files, we'll just collect all the added lines
            for line in lines.iter().skip(1) {
                if line.starts_with('+') {
                    new_content.push_str(&line[1..]);
                    new_content.push('\n');
                }
            }
        } else {
            // For existing files, read the current content
            old_content = fs::read_to_string(&file_path)
                .map_err(|_| anyhow!(DiffError::FileNotFound(file_path.to_string_lossy().to_string())))?;
            
            // Apply the diff to get the new content
            new_content = old_content.clone();
            
            // Process diff lines
            for line in lines.iter().skip(1) {
                if line.starts_with('-') {
                    // Line to remove
                    let line_content = &line[1..];
                    if let Some(pos) = new_content.find(line_content) {
                        let start = new_content[..pos].rfind('\n').map_or(0, |p| p + 1);
                        let end = pos + line_content.len();
                        new_content.replace_range(start..end, "");
                    }
                } else if line.starts_with('+') {
                    // Line to add
                    let line_content = &line[1..];
                    new_content.push_str(line_content);
                    new_content.push('\n');
                }
                // Skip context lines
            }
        }
        
        Ok(FileDiff {
            file_path,
            old_content,
            new_content,
            is_new_file,
        })
    }
}