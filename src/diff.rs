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

impl FileDiff {
    pub fn get_file_path(&self) -> &PathBuf {
        &self.file_path
    }
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
        use colored::*;
        let path_str = self.file_path.to_string_lossy();

        if self.is_new_file {
            format!("New file: {}\n{}", path_str, self.new_content.green())
        } else {
            let diff = TextDiff::from_lines(&self.old_content, &self.new_content);

            let mut diff_output = format!("File: {}\n", path_str);

            for op in diff.ops() {
                for change in diff.iter_changes(op) {
                    match change.tag() {
                        ChangeTag::Delete => {
                            // White text on red background for removed lines
                            let colored_text = change.value().white().on_red().to_string();
                            diff_output.push_str(&colored_text);
                        },
                        ChangeTag::Insert => {
                            // White text on green background for added lines
                            let colored_text = change.value().white().on_green().to_string();
                            diff_output.push_str(&colored_text);
                        },
                        ChangeTag::Equal => continue,
                    };
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
        // More flexible regex that can match various diff formats
        // It captures content between ```diff and ``` even if there's whitespace
        // Also tries to match ```\npath/to/file.ext patterns without the "diff" marker
        let diff_regex = Regex::new(r"```(?:diff)?\s*\n((?:.|\n)*?)```").unwrap();
        Self { diff_regex }
    }

    pub fn extract_raw_diff_blocks(&self, text: &str) -> Vec<String> {
        let mut blocks = Vec::new();

        for captures in self.diff_regex.captures_iter(text) {
            if let Some(diff_text) = captures.get(1) {
                blocks.push(diff_text.as_str().to_string());
            }
        }

        blocks
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

        // Sanitize the file path to ensure it's relative to the current directory
        let mut file_path = PathBuf::from(lines[0].trim());
        
        // Remove any leading slashes or path traversal attempts
        if file_path.is_absolute() || lines[0].trim().starts_with("/") || 
           lines[0].trim().starts_with("\\") || lines[0].trim().starts_with("..") {
            // Convert to a relative path by taking just the file name
            if let Some(file_name) = file_path.file_name() {
                file_path = PathBuf::from(file_name);
            } else {
                return Err(anyhow!(DiffError::InvalidFormat(
                    format!("Invalid file path: {}", lines[0].trim())
                )));
            }
        }
        
        // Ensure the path doesn't try to navigate outside the current directory
        for component in file_path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(anyhow!(DiffError::InvalidFormat(
                    format!("Path traversal attempt detected: {}", lines[0].trim())
                )));
            }
        }
        
        // Get current directory to ensure all operations are relative
        let current_dir = std::env::current_dir()
            .map_err(|_| anyhow!("Failed to get current directory"))?;
        
        // Combine with current directory to get the full path
        let full_path = current_dir.join(&file_path);
        
        // Use the sanitized path for all operations
        let is_new_file = !full_path.exists();

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
            old_content = fs::read_to_string(&full_path)
                .map_err(|_| anyhow!(DiffError::FileNotFound(full_path.to_string_lossy().to_string())))?;

            // Track our position in the document as we process diff lines
            let mut removed_lines = Vec::new();
            let mut added_lines = Vec::new();

            // First, collect all removed and added lines
            for line in lines.iter().skip(1) {
                if line.starts_with('-') {
                    removed_lines.push(&line[1..]);
                } else if line.starts_with('+') {
                    added_lines.push(&line[1..]);
                }
            }

            // Simple approach: replace the old content with the new content
            // by finding the removed lines and replacing them with added lines
            let old_lines: Vec<&str> = old_content.lines().collect();
            let mut new_lines = Vec::new();
            
            let mut i = 0;
            while i < old_lines.len() {
                // Try to find a sequence of removed lines starting at this position
                let mut match_length = 0;
                for (j, &removed) in removed_lines.iter().enumerate() {
                    if i + j < old_lines.len() && old_lines[i + j] == removed {
                        match_length += 1;
                    } else {
                        break;
                    }
                }

                if match_length > 0 && match_length == removed_lines.len() {
                    // Found all removed lines in sequence, replace with added lines
                    for &added in &added_lines {
                        new_lines.push(added.to_string());
                    }
                    i += match_length;
                } else {
                    // No match, keep the original line
                    new_lines.push(old_lines[i].to_string());
                    i += 1;
                }
            }

            new_content = new_lines.join("\n");
            // Add trailing newline if original had one
            if old_content.ends_with('\n') {
                new_content.push('\n');
            }
        }

        Ok(FileDiff {
            file_path: full_path,
            old_content,
            new_content,
            is_new_file,
        })
    }
}
