use anyhow::{Result, anyhow};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Configuration structure for code-llm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default system prompt to use when no model-specific prompt is available
    #[serde(default = "default_system_prompt")]
    pub default_system_prompt: String,
    
    /// Model-specific system prompts
    #[serde(default)]
    pub model_prompts: HashMap<String, String>,
}

/// Get the default system prompt for Ollama models
fn default_system_prompt() -> String {
    "You are a helpful assistant for software development. When suggesting changes to code:

1. ALWAYS present code edits as diff blocks with this EXACT format:
```diff
path/to/file.ext
- old line
+ new line
```

2. IMPORTANT RULES for code suggestions:
   - Include file path on the FIRST line of EACH diff block
   - Start a NEW diff block for EACH file you modify
   - Use complete paths starting from the repository root
   - Show '-' for lines to remove, '+' for lines to add
   - Include enough context lines for changes to be located
   - Use a SEPARATE diff block for EACH distinct change to the same file

3. For new files, use this format:
```diff
path/to/newfile.ext
+ line 1 of new file
+ line 2 of new file
```

4. ALWAYS show diffs for ANY code changes you suggest. Do not just describe changes. Show actual diff blocks.

5. If supplying lengthy code, break it into MULTIPLE small diff blocks rather than one huge block.".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_system_prompt: default_system_prompt(),
            model_prompts: HashMap::new(),
        }
    }
}

impl Config {
    /// Get the system prompt for a specific model
    pub fn get_system_prompt(&self, model: &str) -> &str {
        // First try to get model-specific prompt
        if let Some(prompt) = self.model_prompts.get(model) {
            return prompt;
        }
        
        // Fall back to default prompt
        &self.default_system_prompt
    }
    
    /// Save the configuration to the config file
    pub fn save(&self) -> Result<()> {
        let config_path = get_config_path()?;
        let config_str = toml::to_string_pretty(self)?;
        fs::write(config_path, config_str)?;
        Ok(())
    }
}

/// Get the path to the configuration directory
pub fn get_config_dir() -> Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
    path.push(".code-llm");
    
    // Create the .code-llm directory if it doesn't exist
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    
    Ok(path)
}

/// Get the path to the configuration file
pub fn get_config_path() -> Result<PathBuf> {
    let mut path = get_config_dir()?;
    path.push("config.toml");
    Ok(path)
}

/// Load configuration from file, creating default if it doesn't exist
pub fn load_config() -> Result<Config> {
    let config_path = get_config_path()?;
    
    // If config file exists, load it
    if config_path.exists() {
        let config_str = fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&config_str)?;
        return Ok(config);
    }
    
    // Create and save default config
    let default_config = Config::default();
    default_config.save()?;
    
    Ok(default_config)
}