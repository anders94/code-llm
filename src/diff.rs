use anyhow::{anyhow, Context as AnyhowContext, Result};
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use similar::{ChangeTag, TextDiff};

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
        
        // Convert the file path to a sanitized path relative to current directory
        // We need to handle both absolute paths and paths relative to project root
        let target_path = if self.file_path.is_absolute() {
            // If it's an absolute path, try to make it relative to current directory
            match self.file_path.strip_prefix("/") {
                Ok(rel_path) => current_dir.join(rel_path),
                Err(_) => self.file_path.clone() // Keep as is if we can't strip prefix
            }
        } else {
            // It's already a relative path, join with current directory
            current_dir.join(&self.file_path)
        };
        
        println!("Applying changes to: {}", target_path.display());
        
        if self.is_new_file {
            // For new files, create directories if needed and write the content
            if let Some(parent) = target_path.parent() {
                ensure_directory_exists(parent)?;
            }
            
            fs::write(&target_path, &self.new_content)
                .with_context(|| format!("Failed to write to new file: {:?}", target_path))?;
        } else {
            // For existing files, verify they exist and handle fallbacks
            let actual_path = Self::find_actual_file_path(&target_path, &current_dir)?;
            
            // Write the new content to the file
            fs::write(&actual_path, &self.new_content)
                .with_context(|| format!("Failed to write to file: {:?}", actual_path))?;
        }

        Ok(())
    }

    fn display_diff(&self) -> String {
        // Get the full file path for display
        let file_path_str = self.file_path
            .to_string_lossy()
            .to_string();

        if self.is_new_file {
            // For new files, use standard unified diff format
            let mut diff_output = format!("--- /dev/null\n+++ {}\n", file_path_str);
            diff_output.push_str("@@ -0,0 +1,");
            let new_lines_count = self.new_content.lines().count();
            diff_output.push_str(&format!("{} @@\n", new_lines_count));
            
            // Add each line prefixed with + and with green background
            for line in self.new_content.lines() {
                let display_line = format!("+{}", line);
                diff_output.push_str(&display_line.white().on_green().bold().to_string());
                diff_output.push('\n');
            }
            
            diff_output
        } else {
            // Use similar crate to generate accurate line-by-line differences
            let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
            
            // Start with the standard diff header
            let mut diff_output = format!("--- {}\n+++ {}\n", file_path_str, file_path_str);
            
            // Track the current position in the file
            let mut old_line_num = 1;
            let mut new_line_num = 1;
            
            // Process the diff operations
            for op in diff.ops() {
                // Track start position for this hunk
                let hunk_old_start = old_line_num;
                let hunk_new_start = new_line_num;
                
                // Get the changes for context display
                let changes: Vec<_> = diff.iter_changes(op).collect();
                let mut old_count = 0;
                let mut new_count = 0;
                
                // Count the number of lines in this hunk
                for change in &changes {
                    match change.tag() {
                        ChangeTag::Delete => {
                            old_count += 1;
                        },
                        ChangeTag::Insert => {
                            new_count += 1;
                        },
                        ChangeTag::Equal => {
                            old_count += 1;
                            new_count += 1;
                        },
                    }
                }
                
                // Only show output if there are actual changes
                if old_count > 0 || new_count > 0 {
                    // Add the hunk header with line numbers
                    diff_output.push_str(&format!("@@ -{},{} +{},{} @@\n", 
                        hunk_old_start, old_count, hunk_new_start, new_count));
                    
                    // Output the change lines with appropriate prefixes
                    for change in changes {
                        match change.tag() {
                            ChangeTag::Delete => {
                                // Removed line with - prefix and red background
                                let value = change.value();
                                let display_value = format!("-{}", value);
                                diff_output.push_str(&display_value.white().on_red().bold().to_string());
                                diff_output.push('\n');
                                
                                // Increment the old line counter
                                old_line_num += 1;
                            },
                            ChangeTag::Insert => {
                                // Added line with + prefix and green background
                                let value = change.value();
                                let display_value = format!("+{}", value);
                                diff_output.push_str(&display_value.white().on_green().bold().to_string());
                                diff_output.push('\n');
                                
                                // Increment the new line counter
                                new_line_num += 1;
                            },
                            ChangeTag::Equal => {
                                // Context line with space prefix (no background)
                                let value = change.value();
                                let display_value = format!(" {}", value);
                                diff_output.push_str(&display_value);
                                diff_output.push('\n');
                                
                                // Increment both counters for unchanged lines
                                old_line_num += 1;
                                new_line_num += 1;
                            },
                        };
                    }
                }
            }

            diff_output
        }
    }
}

impl FileDiff {
    // Helper to find the actual file path, with fallbacks
    fn find_actual_file_path(target_path: &Path, current_dir: &Path) -> Result<PathBuf> {
        if target_path.exists() {
            return Ok(target_path.to_path_buf());
        }
        
        // Fallback to just using the filename
        if let Some(file_name) = target_path.file_name() {
            let fallback_path = current_dir.join(file_name);
            
            if fallback_path.exists() {
                println!("Using fallback path: {}", fallback_path.display());
                return Ok(fallback_path);
            }
            
            // If even the fallback doesn't exist, return an error with both paths we tried
            return Err(anyhow!(DiffError::FileNotFound(
                format!("Tried: {} and {}", 
                    target_path.to_string_lossy(), 
                    fallback_path.to_string_lossy())
            )));
        }
        
        Err(anyhow!("Invalid file path"))
    }
}

pub struct DiffGenerator {
    diff_regex: Regex,
}

impl DiffGenerator {
    pub fn new() -> Self {
        // Match any code block with optional language tag
        let diff_regex = Regex::new(r"```(?:[a-zA-Z0-9_\-+.]*)?(?:\s*\n|\s)((?:.|\n)*?)```").unwrap();
        Self { diff_regex }
    }
    
    pub fn extract_raw_diff_blocks(&self, text: &str) -> Vec<String> {
        // First try to extract code blocks with triple backticks
        let markdown_blocks = self.extract_code_blocks(text);
        if !markdown_blocks.is_empty() {
            return markdown_blocks;
        }
        
        // Try to parse as raw diff text if no code blocks were found
        vec![text.to_string()]
    }
    
    // Extract code blocks with triple backticks
    fn extract_code_blocks(&self, text: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        
        for captures in self.diff_regex.captures_iter(text) {
            if let Some(block_match) = captures.get(1) {
                let block = block_match.as_str().to_string();
                
                // Only include the block if it looks like a diff
                if self.is_likely_diff(&block) {
                    blocks.push(block);
                }
            }
        }
        
        blocks
    }
    
    // Check if a block is likely a diff
    fn is_likely_diff(&self, text: &str) -> bool {
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return false;
        }
        
        // Check for common diff markers
        let has_diff_header = lines.iter().any(|line| line.starts_with("--- ") || line.starts_with("+++ "));
        let has_hunk_header = lines.iter().any(|line| line.starts_with("@@ -"));
        let has_plus_minus = lines.iter().any(|line| line.starts_with('+') || line.starts_with('-'));
        
        // Check if it's an explicitly marked diff block
        let is_diff_format = text.trim().starts_with("diff ") || 
                             (text.contains("--- ") && text.contains("+++ "));
        
        has_diff_header || has_hunk_header || (has_plus_minus && lines.len() > 2) || is_diff_format
    }
    
    pub fn extract_diffs(&self, text: &str) -> Vec<FileDiff> {
        let mut diffs = Vec::new();
        
        // Get all potential diff blocks
        let diff_blocks = self.extract_raw_diff_blocks(text);
        
        // Try to parse each block as a diff
        for block in diff_blocks {
            if let Ok(diff) = self.parse_diff(&block) {
                diffs.push(diff);
            }
        }
        
        diffs
    }
    
    fn parse_diff(&self, diff_text: &str) -> Result<FileDiff> {
        // Extract file path and content from the diff
        let lines: Vec<&str> = diff_text.lines().collect();
        
        if lines.is_empty() {
            return Err(anyhow!(DiffError::InvalidFormat("Diff is empty".to_string())));
        }
        
        // Extract file paths from unified diff headers
        let mut file_path = PathBuf::new();
        let mut is_new_file = false;
        
        for line in &lines {
            if line.starts_with("--- ") {
                let source_path = line.trim_start_matches("--- ");
                if source_path == "/dev/null" {
                    is_new_file = true;
                }
            } else if line.starts_with("+++ ") {
                let path_part = line.trim_start_matches("+++ ");
                
                // Sanitize the path
                let clean_path = path_part.trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim();
                
                if clean_path != "/dev/null" {
                    // Clean up common prefixes (a/, b/, etc.)
                    let final_path = clean_path
                        .trim_start_matches("a/")
                        .trim_start_matches("b/")
                        .trim_start_matches("./");
                        
                    file_path = PathBuf::from(final_path);
                    break;
                }
            }
        }
        
        // If we couldn't find a path in headers, try the first line or look for filenames
        if file_path.as_os_str().is_empty() {
            let first_line = lines[0].trim();
            
            if !first_line.starts_with('+') && !first_line.starts_with('-') && 
               !first_line.starts_with('@') && !first_line.contains(" ") {
                // First line might be a filename
                file_path = PathBuf::from(first_line);
            } else {
                // Search for a filename in the first few lines
                for line in lines.iter().take(5) {
                    let cleaned = line.trim();
                    if (cleaned.contains('.') || cleaned.contains('/')) && 
                       !cleaned.starts_with('+') && !cleaned.starts_with('-') && 
                       !cleaned.starts_with('@') && !cleaned.contains(" ") {
                        file_path = PathBuf::from(cleaned);
                        break;
                    }
                }
            }
        }
        
        if file_path.as_os_str().is_empty() {
            return Err(anyhow!(DiffError::InvalidFormat("Could not determine file path from diff".to_string())));
        }
        
        println!("Parsed file path: {}", file_path.display());
        
        // Check if the file exists if we're not sure it's a new file
        if !is_new_file {
            let current_dir = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
                
            let full_path = current_dir.join(&file_path);
            
            // If the path doesn't exist, check just the filename
            if !full_path.exists() {
                let file_name_only = file_path.file_name().unwrap_or_default();
                let file_name_path = current_dir.join(file_name_only);
                
                is_new_file = !file_name_path.exists();
            } else {
                is_new_file = false;
            }
        }
        
        // Get old content for existing files
        let old_content = if is_new_file {
            String::new()
        } else {
            let current_dir = std::env::current_dir()
                .map_err(|_| anyhow!("Failed to get current directory"))?;
                
            let target_path = current_dir.join(&file_path);
            
            // Try to read the file with fallbacks
            match fs::read_to_string(&target_path) {
                Ok(content) => content,
                Err(_) => {
                    // Try just the filename
                    if let Some(file_name) = file_path.file_name() {
                        let fallback_path = current_dir.join(file_name);
                        
                        match fs::read_to_string(&fallback_path) {
                            Ok(content) => content,
                            Err(_) => {
                                return Err(anyhow!(DiffError::FileNotFound(
                                    format!("Could not find file at any of: {}, {}", 
                                        target_path.display(), fallback_path.display())
                                )));
                            }
                        }
                    } else {
                        return Err(anyhow!(DiffError::FileNotFound(
                            format!("Invalid file path: {}", file_path.display())
                        )));
                    }
                }
            }
        };
        
        // Extract new content from the diff
        let new_content = if is_new_file {
            // For new files, extract all lines that start with +
            let mut content = String::new();
            let mut in_hunk = false;
            
            for line in &lines {
                if line.starts_with("@@ ") {
                    in_hunk = true;
                    continue;
                }
                
                if (in_hunk || !line.starts_with("---") && !line.starts_with("+++")) && 
                   line.starts_with('+') && !line.starts_with("+++ ") {
                    // Remove the + prefix
                    content.push_str(&line[1..]);
                    content.push('\n');
                }
            }
            
            content
        } else {
            // For existing files, apply the diff to the original content
            let old_lines: Vec<&str> = old_content.lines().collect();
            let mut new_lines = old_lines.iter().map(|&s| s.to_string()).collect::<Vec<String>>();
            
            // Process hunks with line numbers
            let mut i = 0;
            while i < lines.len() {
                let line = lines[i];
                
                // Look for hunk headers
                if line.starts_with("@@ -") && line.contains(" @@") {
                    // Parse the hunk header
                    let header_parts: Vec<&str> = line
                        .trim_matches(|c| c == '@' || c == ' ')
                        .split(' ')
                        .collect();
                    
                    if header_parts.len() >= 2 {
                        let old_info = header_parts[0].trim_start_matches('-');
                        let _new_info = header_parts[1].trim_start_matches('+');
                        
                        // Parse old line numbers: -X,Y where X = start line (1-based), Y = line count
                        let old_parts: Vec<&str> = old_info.split(',').collect();
                        if old_parts.len() >= 1 {
                            let old_start = old_parts[0].parse::<usize>().unwrap_or(1);
                            let old_count = if old_parts.len() >= 2 {
                                old_parts[1].parse::<usize>().unwrap_or(0)
                            } else {
                                0
                            };
                            
                            // Collect hunk content
                            let mut old_hunk_content = Vec::new();
                            let mut new_hunk_content = Vec::new();
                            
                            // Move to content lines
                            i += 1;
                            while i < lines.len() {
                                let hunk_line = lines[i];
                                
                                if hunk_line.starts_with('-') {
                                    old_hunk_content.push(&hunk_line[1..]);
                                } else if hunk_line.starts_with('+') {
                                    new_hunk_content.push(&hunk_line[1..]);
                                } else if hunk_line.starts_with(' ') {
                                    // Context lines are the same in both
                                    old_hunk_content.push(&hunk_line[1..]);
                                    new_hunk_content.push(&hunk_line[1..]);
                                } else if hunk_line.starts_with("@@ ") {
                                    // Next hunk header
                                    i -= 1;
                                    break;
                                } else if hunk_line.is_empty() {
                                    // Skip empty lines but continue
                                } else {
                                    // End of hunk
                                    break;
                                }
                                
                                i += 1;
                            }
                            
                            // Apply changes to new_lines
                            let old_start_idx = old_start.saturating_sub(1); // Convert to 0-based
                            let old_range_end = old_start_idx + old_count;
                            
                            if old_start_idx < new_lines.len() {
                                let capped_range_end = std::cmp::min(old_range_end, new_lines.len());
                                
                                // Replace the old lines with new lines
                                new_lines.splice(
                                    old_start_idx..capped_range_end,
                                    new_hunk_content.iter().map(|&s| s.to_string())
                                );
                            }
                        }
                    }
                }
                
                i += 1;
            }
            
            // If standard hunk parsing failed, try simpler approach
            if new_lines.iter().map(|s| s.as_str()).collect::<Vec<&str>>() == old_lines {
                // Collect removed and added lines
                let mut removed_lines = Vec::new();
                let mut added_lines = Vec::new();
                
                for line in &lines {
                    if line.starts_with('-') && !line.starts_with("--- ") {
                        removed_lines.push(&line[1..]);
                    } else if line.starts_with('+') && !line.starts_with("+++ ") {
                        added_lines.push(&line[1..]);
                    }
                }
                
                // Apply the changes
                if !removed_lines.is_empty() || !added_lines.is_empty() {
                    let mut result = Vec::new();
                    let mut i = 0;
                    
                    while i < old_lines.len() {
                        // Try to find a sequence of removed lines at this position
                        if i <= old_lines.len() - removed_lines.len() {
                            let mut matched = true;
                            for (j, &removed) in removed_lines.iter().enumerate() {
                                if i + j >= old_lines.len() || old_lines[i + j] != removed {
                                    matched = false;
                                    break;
                                }
                            }
                            
                            if matched {
                                // Replace removed lines with added lines
                                for &added in &added_lines {
                                    result.push(added.to_string());
                                }
                                i += removed_lines.len();
                                continue;
                            }
                        }
                        
                        // No match, keep original line
                        result.push(old_lines[i].to_string());
                        i += 1;
                    }
                    
                    new_lines = result;
                }
            }
            
            // Combine the lines
            let mut content = new_lines.join("\n");
            
            // Add trailing newline if original had one
            if old_content.ends_with('\n') {
                content.push('\n');
            }
            
            content
        };
        
        Ok(FileDiff {
            file_path,
            old_content,
            new_content,
            is_new_file,
        })
    }
}