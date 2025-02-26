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
        // Get current directory
        let current_dir = std::env::current_dir()
            .map_err(|_| anyhow!("Failed to get current directory"))?;
            
        // Extract the filename from the file_path, ensuring it's relative to current directory
        let file_name = if let Some(name) = self.file_path.file_name() {
            PathBuf::from(name)
        } else {
            return Err(anyhow!("Invalid file path"));
        };
        
        let target_path = current_dir.join(&file_name);
        
        if self.is_new_file {
            // For new files, create directories if needed and write the content
            if let Some(parent) = target_path.parent() {
                ensure_directory_exists(parent)?;
            }
            
            fs::write(&target_path, &self.new_content)
                .with_context(|| format!("Failed to write to new file: {:?}", target_path))?;
        } else {
            // For existing files, verify they exist
            if !target_path.exists() {
                return Err(anyhow!(DiffError::FileNotFound(
                    target_path.to_string_lossy().to_string()
                )));
            }
            
            // Write the new content to the file
            fs::write(&target_path, &self.new_content)
                .with_context(|| format!("Failed to write to file: {:?}", target_path))?;
        }

        Ok(())
    }

    fn display_diff(&self) -> String {
        use colored::*;
        
        // Get just the filename part for display
        let file_name = self.file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown file".to_string());

        if self.is_new_file {
            format!("New file: {}\n{}", file_name, self.new_content.green())
        } else {
            let diff = TextDiff::from_lines(&self.old_content, &self.new_content);

            let mut diff_output = format!("File: {}\n", file_name);

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
        
        // Extract a clean filename from the first line, ignoring diff markers
        let first_line = lines[0].trim();
        let file_path_str = first_line
            .trim_start_matches('+')
            .trim_start_matches('-')
            .trim_start_matches("// ")
            .trim_start_matches("/* ")
            .trim_start_matches("* ")
            .trim_start_matches("/*")
            .trim_start_matches('/')
            .trim_start_matches('\\')
            .trim();
            
        // Build a simple path, we'll sanitize it when applying
        let file_path = PathBuf::from(file_path_str);
        
        // Check if the file exists in the current directory
        let is_new_file = !std::path::Path::new(file_path.file_name().unwrap_or_default()).exists();

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
            // Get the path to the current file in the working directory
            let actual_path = std::path::Path::new(file_path.file_name().unwrap_or_default());
            
            // For existing files, read the current content
            old_content = fs::read_to_string(actual_path)
                .map_err(|_| anyhow!(DiffError::FileNotFound(actual_path.to_string_lossy().to_string())))?;

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
            file_path,
            old_content,
            new_content,
            is_new_file,
        })
    }
}
