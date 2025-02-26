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
            // For new files, highlight the entire content with bright green background
            let mut diff_output = format!("New file: {}\n", file_name);
            
            // Add each line prefixed with + and with green background
            for line in self.new_content.lines() {
                let display_line = if line.trim().is_empty() {
                    line.to_string()
                } else {
                    format!("+{}", line)
                };
                diff_output.push_str(&display_line.white().on_green().bold().to_string());
                diff_output.push('\n');
            }
            
            diff_output
        } else {
            let diff = TextDiff::from_lines(&self.old_content, &self.new_content);

            let mut diff_output = format!("File: {}\n", file_name);

            for op in diff.ops() {
                for change in diff.iter_changes(op) {
                    match change.tag() {
                        ChangeTag::Delete => {
                            // White text on red background for removed lines
                            let value = change.value();
                            // Make sure we prefix with - for clarity in terminals that don't support colors
                            let display_value = if !value.starts_with('-') && !value.trim().is_empty() {
                                format!("-{}", value)
                            } else {
                                value.to_string()
                            };
                            // Apply coloring: white text on dark red background for better readability
                            let colored_text = display_value.white().on_red().bold().to_string();
                            diff_output.push_str(&colored_text);
                        },
                        ChangeTag::Insert => {
                            // White text on green background for added lines
                            let value = change.value();
                            // Make sure we prefix with + for clarity in terminals that don't support colors
                            let display_value = if !value.starts_with('+') && !value.trim().is_empty() {
                                format!("+{}", value)
                            } else {
                                value.to_string()
                            };
                            // Apply coloring: white text on dark green background for better readability
                            let colored_text = display_value.white().on_green().bold().to_string();
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
        // Match any code block with optional language tag
        // This will match everything between triple backticks
        // - Handles explicit diff blocks: ```diff\n...```
        // - Handles code blocks with language tags: ```js\n...```
        // - Handles generic code blocks: ```\n...```
        // - Even matches code blocks with leading whitespace
        let diff_regex = Regex::new(r"```(?:[a-zA-Z0-9_\-+.]*)?(?:\s*\n|\s)((?:.|\n)*?)```").unwrap();
        Self { diff_regex }
    }
    
    // Helper method to extract actual code content from a response containing [Pasted text +N lines]
    fn extract_actual_code_content(&self, diff_text: &str) -> Option<String> {
        // If this is a special marker for pasted text, try to find the actual code
        let pasted_text_regex = Regex::new(r"\[Pasted text \+(\d+) lines\]").ok()?;
        
        // Check if we're dealing with a pasted text marker
        if pasted_text_regex.is_match(diff_text) {
            // Extract the entire response text to look for code blocks
            let code_block_regex = Regex::new(r"```(?:[a-zA-Z0-9_\-+.]*)?(?:\s*\n|\s)((?:.|\n)*?)```").ok()?;
            
            // Find the largest code block in the text
            let mut largest_block = String::new();
            let mut largest_size = 0;
            
            for capture in code_block_regex.captures_iter(diff_text) {
                if let Some(code) = capture.get(1) {
                    let code_str = code.as_str();
                    let code_lines = code_str.lines().count();
                    
                    if code_lines > largest_size {
                        largest_size = code_lines;
                        largest_block = code_str.to_string();
                    }
                }
            }
            
            if !largest_block.is_empty() {
                return Some(largest_block);
            }
            
            // If we couldn't find code blocks, look for indented text blocks
            let lines: Vec<&str> = diff_text.lines().collect();
            let mut in_code_block = false;
            let mut code_block = String::new();
            let mut code_lines = 0;
            
            for line in lines {
                // Indented lines are likely code
                if line.starts_with("    ") || line.starts_with("\t") {
                    if !in_code_block {
                        in_code_block = true;
                    }
                    // Remove the indentation
                    let trimmed = if line.starts_with("    ") { &line[4..] } else { &line[1..] };
                    code_block.push_str(trimmed);
                    code_block.push('\n');
                    code_lines += 1;
                } else if in_code_block && line.trim().is_empty() {
                    // Empty line within code block
                    code_block.push('\n');
                } else if in_code_block {
                    // End of code block
                    in_code_block = false;
                    
                    if code_lines > largest_size {
                        largest_size = code_lines;
                        largest_block = code_block.clone();
                    }
                    
                    code_block.clear();
                    code_lines = 0;
                }
            }
            
            // Check if we ended with a code block
            if in_code_block && code_lines > largest_size {
                largest_block = code_block;
            }
            
            if !largest_block.is_empty() {
                return Some(largest_block);
            }
        }
        
        None
    }
    
    // Helper method to check if a block is likely a diff
    fn is_diff_block(&self, block: &str) -> bool {
        // Check for common diff markers
        let lines: Vec<&str> = block.lines().collect();
        if lines.is_empty() {
            return false;
        }
        
        // 1. Special handling for explicitly marked diff blocks
        if block.trim().starts_with("```diff") {
            return true;
        }
        
        // 2. Special handling for pasted text blocks
        if block.contains("[Pasted text +") {
            return true;
        }
        
        // 3. If the first line is a filename
        let first_line = lines[0].trim();
        
        // Common file extensions to recognize
        let common_extensions = [
            ".js", ".jsx", ".ts", ".tsx", ".py", ".rs", ".java", ".c", ".cpp", ".h", 
            ".go", ".rb", ".php", ".html", ".css", ".json", ".md", ".yml", ".yaml", 
            ".xml", ".txt", ".sh", ".bash", ".ps1", ".cs", ".fs", ".swift", ".kt",
            ".dart", ".lua", ".pl", ".r", ".scala", ".sql", ".conf", ".ini", ".toml"
        ];
        
        // Check if the first line looks like a filename
        let looks_like_file_path = 
            // Ends with a common file extension
            common_extensions.iter().any(|ext| first_line.ends_with(ext)) || 
            // Contains path separators
            first_line.contains("/") || first_line.contains("\\") ||
            // Contains common keywords
            first_line.contains("diff") || first_line.contains("patch") ||
            // Is just a filename without any other text
            (first_line.contains(".") && !first_line.contains(" "));
        
        // 4. Check for + and - patterns
        
        // Simple + and - count
        let plus_count = lines.iter().filter(|line| line.trim_start().starts_with('+')).count();
        let minus_count = lines.iter().filter(|line| line.trim_start().starts_with('-')).count();
        
        // If the first line looks like a path and we have diff markers, it's likely a diff
        if looks_like_file_path && (plus_count > 0 || minus_count > 0) {
            return true;
        }
        
        // Even without a file path, strong evidence of a diff pattern
        if plus_count >= 2 || minus_count >= 2 || (plus_count >= 1 && minus_count >= 1) {
            return true;
        }
        
        // 5. Special format detection: Look for diff formatting patterns
        
        // Pattern matching for specific diff formatting
        let has_diff_pattern = lines.windows(3).any(|window| {
            // Check for typical diff pattern where lines have +/- at the same position
            if window.len() >= 2 {
                let line1 = window[0].trim_start();
                let line2 = window[1].trim_start();
                
                // Check if we have consecutive lines with + and - at the same column position
                (line1.starts_with('+') && line2.starts_with('-')) ||
                (line1.starts_with('-') && line2.starts_with('+'))
            } else {
                false
            }
        });
        
        if has_diff_pattern {
            return true;
        }
        
        // 6. Last resort: If there's exactly one + and one - line, and the block is short 
        // (like the example given), it's probably a diff
        if plus_count == 1 && minus_count == 1 && lines.len() < 15 {
            return true;
        }
        
        // 7. Check for partial code blocks that might be additions
        let looks_like_pasted_code = 
            // Contains indentation patterns typical of code
            lines.iter().filter(|line| line.starts_with("  ") || line.starts_with("\t")).count() > 2 ||
            // Contains common code patterns
            lines.iter().any(|line| 
                line.contains("function") || 
                line.contains("class") || 
                line.contains("import") || 
                line.contains("const") || 
                line.contains("let") || 
                line.contains("var") ||
                line.contains("def ") ||
                line.contains("return ") ||
                line.contains(" = ")
            );
            
        if looks_like_pasted_code && lines.len() > 3 {
            // This looks like a code block, let's treat it as a diff
            return true;
        }
        
        // Default to false if none of the above conditions are met
        false
    }

    pub fn extract_raw_diff_blocks(&self, text: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        
        // Special handling for pasted text blocks
        if text.contains("[Pasted text +") {
            // Extract blocks that match the pattern [Pasted text +N lines]
            let pasted_text_regex = Regex::new(r"\[Pasted text \+(\d+) lines\]").unwrap();
            
            // First, see if we can find any pasted text blocks
            if pasted_text_regex.is_match(text) {
                // If there's a filename or path mentioned before the pasted text
                let lines: Vec<&str> = text.lines().collect();
                let mut filename = "pasted_code.txt"; // Default filename
                
                // Try to find a filename in the text
                for line in &lines {
                    if line.contains(".") && (
                        line.ends_with(".js") || 
                        line.ends_with(".py") || 
                        line.ends_with(".ts") || 
                        line.ends_with(".rs") || 
                        line.ends_with(".java") ||
                        line.ends_with(".c") || 
                        line.ends_with(".cpp") ||
                        line.contains("/") || 
                        line.contains("\\")
                    ) {
                        filename = line.trim();
                        break;
                    }
                }
                
                // Create a block with the filename and a placeholder for the pasted content
                let mut diff_block = String::new();
                diff_block.push_str(filename);
                diff_block.push('\n');
                diff_block.push_str("+ [Entire file content is new/modified]\n");
                
                blocks.push(diff_block);
                return blocks;
            }
        }
        
        // First check for raw text directly (sometimes models don't use proper markdown format)
        let raw_lines: Vec<&str> = text.lines().collect();
        if raw_lines.len() > 5 {
            // If we have substantial text with + and - patterns outside code blocks,
            // then try to process it as a raw diff
            let plus_count = raw_lines.iter().filter(|line| line.trim_start().starts_with('+')).count();
            let minus_count = raw_lines.iter().filter(|line| line.trim_start().starts_with('-')).count();
            
            if plus_count >= 2 && minus_count >= 1 {
                // This could be a raw diff - extract all consecutive lines starting with + or -
                let mut current_block = String::new();
                let mut in_diff = false;
                
                // Pre-process: try to find a potential filename before processing the lines
                let mut potential_filename = String::new();
                for line in &raw_lines {
                    let trimmed = line.trim();
                    if trimmed.contains(".") && !trimmed.contains(" ") && !trimmed.starts_with('+') && !trimmed.starts_with('-') {
                        potential_filename = trimmed.to_string();
                        break;
                    }
                }
                
                for line in &raw_lines {
                    if line.trim_start().starts_with('+') || line.trim_start().starts_with('-') {
                        if !in_diff {
                            in_diff = true;
                            // Add the potential filename if we found one
                            if !potential_filename.is_empty() {
                                current_block.push_str(&potential_filename);
                                current_block.push('\n');
                            }
                        }
                        current_block.push_str(line);
                        current_block.push('\n');
                    } else if in_diff && line.trim().is_empty() {
                        // Empty line within diff
                        current_block.push('\n');
                    } else if in_diff {
                        // End of diff section
                        blocks.push(current_block);
                        current_block = String::new();
                        in_diff = false;
                    }
                }
                
                if in_diff {
                    blocks.push(current_block);
                }
            }
            
            // Also check for code block patterns even if they don't have + or - markers
            // This helps handle cases where the code is presented without diff markers
            if plus_count == 0 && minus_count == 0 {
                // Look for patterns that suggest this is code
                let code_line_count = raw_lines.iter().filter(|line| 
                    line.contains("function") || 
                    line.contains("class") || 
                    line.contains("import") || 
                    line.contains("const") || 
                    line.contains("var") || 
                    line.contains("let") ||
                    line.contains("def ") ||
                    line.contains("return ") ||
                    line.trim().starts_with("if ") ||
                    line.trim().starts_with("for ") ||
                    line.trim().ends_with("{") ||
                    line.trim().ends_with(":")
                ).count();
                
                if code_line_count >= 3 {
                    // This looks like code - try to find a filename
                    let mut filename = "unknown_file.txt";
                    for line in &raw_lines {
                        if line.contains(".") && !line.contains(" ") {
                            filename = line.trim();
                            break;
                        }
                    }
                    
                    // Create a synthetic diff block
                    let mut diff_block = String::new();
                    diff_block.push_str(filename);
                    diff_block.push('\n');
                    diff_block.push_str("+ [Entire file content is new/modified]\n");
                    
                    blocks.push(diff_block);
                }
            }
        }
        
        // Then check standard code blocks with triple backticks
        for captures in self.diff_regex.captures_iter(text) {
            if let Some(diff_text) = captures.get(1) {
                let block = diff_text.as_str().to_string();
                
                // Only include blocks that look like diffs (contain + or - lines)
                if self.is_diff_block(&block) {
                    blocks.push(block);
                }
            }
        }

        blocks
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
        // Extract file path from the first line
        let lines: Vec<&str> = diff_text.lines().collect();

        if lines.is_empty() {
            return Err(anyhow!(DiffError::InvalidFormat(
                "Diff is empty".to_string()
            )));
        }
        
        // Special handling for "[Entire file content is new/modified]" marker
        let has_entire_file_marker = diff_text.contains("[Entire file content is new/modified]");
        
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
        let mut is_new_file = !std::path::Path::new(file_path.file_name().unwrap_or_default()).exists();
        
        // If we have the entire file marker, this might be a complete replacement
        // even if the file exists, we'll treat it as a new file
        if has_entire_file_marker {
            is_new_file = true;
        }

        let mut old_content = String::new();
        let mut new_content = String::new();

        if is_new_file {
            // Special handling for "[Entire file content is new/modified]" marker
            if has_entire_file_marker {
                // Try to extract the actual content from the response text
                if let Some(content) = self.extract_actual_code_content(&diff_text) {
                    new_content = content;
                } else {
                    // If we can't extract specific content, provide a placeholder
                    new_content = "// This file was marked as new or completely replaced,\n// but the specific content couldn't be extracted.\n".to_string();
                }
            } else {
                // For regular new files, we'll just collect all the added lines
                for line in lines.iter().skip(1) {
                    if line.starts_with('+') {
                        // Trim leading space after the '+' to handle "+ code" formatting
                        let content = &line[1..];
                        let trimmed = if content.starts_with(' ') { &content[1..] } else { content };
                        new_content.push_str(trimmed);
                        new_content.push('\n');
                    }
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
                    // For removed lines, we need to keep the exact formatting
                    removed_lines.push(&line[1..]);
                } else if line.starts_with('+') {
                    // For added lines, handle the "+ code" formatting by removing extra leading space
                    let content = &line[1..];
                    let trimmed = if content.starts_with(' ') { &content[1..] } else { content };
                    added_lines.push(trimmed);
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
