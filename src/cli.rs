use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::io::{self, Write};

use crate::api::OllamaClient;
use crate::config::{load_config, get_config_dir, get_config_path};
use crate::context::ContextManager;
use crate::diff::{DiffGenerator, DiffAction};

#[derive(Parser)]
#[clap(author, version, about)]
pub struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,

    /// The model to use for code suggestions
    #[clap(short, long)]
    model: Option<String>,

    /// Ollama API endpoint URL
    #[clap(long, default_value = "http://localhost:11434")]
    api_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new context
    Init,
    
    /// Edit the configuration
    Config {
        /// Show the path to the configuration file
        #[clap(short, long)]
        path: bool,
        
        /// Open the configuration file in the default editor
        #[clap(short, long)]
        edit: bool,
    },
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let model_opt = cli.model;
    let api_url = cli.api_url;
    
    // Load configuration
    let config = load_config()?;

    match &cli.command {
        Some(Commands::Init) => {
            println!("{}", "Initializing new context...".green());
            
            // Create local .code-llm directory path
            let local_config_dir = PathBuf::from(".code-llm");
            let local_config_path = local_config_dir.join("config.toml");
            
            // Check if local config already exists
            let should_proceed = if local_config_path.exists() {
                println!("{}", format!("⚠️  Warning: Local config file already exists at {}", local_config_path.display()).yellow());
                println!("{}", "Initializing will overwrite the existing configuration.".yellow());
                
                // Ask user if they want to proceed
                let options = vec!["Yes, overwrite it", "No, cancel initialization"];
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Do you want to proceed?")
                    .default(1) // Default to safer option (cancel)
                    .items(&options)
                    .interact()?;
                
                selection == 0 // True if they selected "Yes"
            } else {
                true // No existing config, proceed
            };
            
            if !should_proceed {
                println!("{}", "Initialization cancelled.".blue());
                return Ok(());
            }
            
            // Check if Ollama is running and select a model
            let selected_model = initialize_with_model_selection(model_opt, &api_url, &config).await?;
            
            // Create directory if needed
            if !local_config_dir.exists() {
                println!("{}", "Creating local .code-llm directory...".blue());
                fs::create_dir_all(&local_config_dir)?;
            }
            
            // Create local config.toml file
            println!("{}", format!("Creating local config file at {}...", local_config_path.display()).blue());
            
            // Create minimal config with selected model
            let config_content = format!(r#"# code-llm local configuration
# Created by code-llm init

# Default model to use
model = "{}"
"#, selected_model);
            
            fs::write(&local_config_path, config_content)?;
            
            println!("{}", format!("✅ Project initialized successfully with model '{}'", selected_model).green());
            println!("{}", "You can now run 'code-llm' in this directory to start the interactive mode.".blue());
            
            return Ok(());
        }
        Some(Commands::Config { path, edit }) => {
            let config_path = get_config_path()?;
            
            if *path {
                // Just show the path to the config file
                println!("{}", config_path.to_string_lossy());
                return Ok(());
            }
            
            if *edit {
                // Try to open the default editor
                #[cfg(target_os = "windows")]
                {
                    std::process::Command::new("notepad")
                        .arg(&config_path)
                        .spawn()?;
                }
                
                #[cfg(not(target_os = "windows"))]
                {
                    // Try to get the default editor from environment variables
                    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                    std::process::Command::new(editor)
                        .arg(&config_path)
                        .spawn()?
                        .wait()?;
                }
                
                println!("{}", format!("Edited configuration at {}", config_path.display()).green());
                return Ok(());
            }
            
            // Default behavior: print the config file contents
            if config_path.exists() {
                let config_content = fs::read_to_string(&config_path)?;
                println!("{}", config_content);
            } else {
                println!("{}", "Configuration file does not exist yet. It will be created when you first run the tool.".yellow());
            }
            return Ok(());
        }
        None => {
            // Interactive mode
            run_interactive_mode(model_opt, &api_url, config).await?;
        }
    }

    Ok(())
}

/// Starts an animated "Thinking..." prompt with cycling dots in a separate thread.
/// Returns a handle to the animation that can be used to stop it.
fn start_thinking_animation() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    
    thread::spawn(move || {
        let mut state = 0;
        let states = [".", "..", "...", "....", "....."];
        
        while running_clone.load(Ordering::SeqCst) {
            // Clear the line and print the current state
            print!("\r{}{:<5}", "Thinking".yellow(), states[state].yellow());
            io::stdout().flush().unwrap();
            
            // Cycle through states
            state = (state + 1) % states.len();
            
            // Wait a bit before updating
            thread::sleep(Duration::from_millis(300));
        }
        
        // Clear the line when done
        print!("\r{:<15}\r", "");
        io::stdout().flush().unwrap();
    });
    
    running
}

/// Stops the thinking animation thread
fn stop_thinking_animation(handle: Arc<AtomicBool>) {
    handle.store(false, Ordering::SeqCst);
    // Small delay to ensure the thread has time to clean up
    thread::sleep(Duration::from_millis(50));
}

/// Handles Ollama connectivity check and model selection
/// Returns the selected model name
async fn initialize_with_model_selection(model_opt: Option<String>, api_url: &str, config: &crate::config::Config) -> Result<String> {
    // Create a temporary client for testing connection and getting models
    let temp_client = OllamaClient::new(api_url, "", config.clone());
    
    // Test connection to Ollama on startup
    println!("{}", "Testing connection to Ollama...".yellow());
    match temp_client.test_connection().await {
        Ok(true) => println!("{}", "✅ Connected to Ollama successfully!".green()),
        Ok(false) => {
            println!("{}", format!("❌ Failed to connect to Ollama at {}. Is Ollama running?", api_url).red());
            println!("{}", "Please start Ollama and try again.".yellow());
            return Err(anyhow!("Could not connect to Ollama"));
        },
        Err(e) => {
            println!("{}", format!("❌ Error testing connection to Ollama: {}", e).red());
            println!("{}", "Please check that Ollama is running and try again.".yellow());
            return Err(anyhow!("Error testing connection to Ollama"));
        }
    }
    
    // Get available models
    let available_models = match temp_client.get_available_models().await {
        Ok(models) => models,
        Err(e) => {
            println!("{}", format!("❌ Error getting available models: {}", e).red());
            return Err(anyhow!("Error getting available models"));
        }
    };
    
    if available_models.is_empty() {
        println!("{}", "❌ No models found in Ollama. Please pull a model first.".red());
        println!("{}", "Example: ollama pull llama3".yellow());
        return Err(anyhow!("No models available"));
    }
    
    // Determine which model to use
    let selected_model = match model_opt {
        Some(model) => {
            // Check if the specified model exists
            println!("{}", format!("Checking if model '{}' is available...", model).yellow());
            
            if available_models.contains(&model) {
                println!("{}", "✅ Model found!".green());
                model
            } else {
                println!("{}", format!("⚠️ Model '{}' not found!", model).yellow());
                select_model_from_list(&available_models)?
            }
        },
        None => {
            // No model specified, ask user to select one
            println!("{}", "No model specified. Please select from available models:".blue());
            select_model_from_list(&available_models)?
        }
    };
    
    Ok(selected_model)
}

async fn run_interactive_mode(model_opt: Option<String>, api_url: &str, config: crate::config::Config) -> Result<()> {
    // Check connectivity and select model
    let selected_model = initialize_with_model_selection(model_opt, api_url, &config).await?;
    
    // Create the client with the selected model
    let client = OllamaClient::new(api_url, &selected_model, config.clone());
    
    let context_manager = ContextManager::new(".")?;
    let diff_generator = DiffGenerator::new();
    
    println!("{}", format!("Welcome to code-llm! Using model: {}", selected_model).green());
    println!("{}", "Type your questions/requests or 'exit' to quit.".blue());
    
    let mut conversation_history = Vec::new();
    let mut current_context = context_manager.get_context()?;
    
    // Set up rustyline for history
    let history_path = get_history_file_path()?;
    let mut rl = DefaultEditor::new()?;
    
    // Load history if the file exists
    if history_path.exists() {
        if let Err(err) = rl.load_history(&history_path) {
            println!("{}", format!("Warning: Failed to load history: {}", err).yellow());
        }
    }
    
    loop {
        // Get user input with history support
        let user_input = match rl.readline("You> ") {
            Ok(line) => {
                // Add valid input to history
                if !line.trim().is_empty() {
                    rl.add_history_entry(&line)?;
                    
                    // Save history after each command
                    if let Err(err) = rl.save_history(&history_path) {
                        println!("{}", format!("Warning: Failed to save history: {}", err).yellow());
                    }
                }
                line
            },
            Err(ReadlineError::Interrupted) => {
                println!("{}", "Interrupted (Ctrl+C)".blue());
                continue;
            },
            Err(ReadlineError::Eof) => {
                println!("{}", "Exiting due to Ctrl+D".blue());
                return Ok(());
            },
            Err(err) => {
                return Err(anyhow!("Error reading input: {}", err));
            }
        };
        
        if user_input.trim().is_empty() {
            // Skip empty inputs
            continue;
        }
        
        if user_input.trim().to_lowercase() == "exit" || user_input.trim().to_lowercase() == "quit" {
            break;
        }
        
        conversation_history.push(format!("User: {}", user_input));
        
        // Start the animated "Thinking..." prompt
        let thinking_handle = start_thinking_animation();
        
        // Get response from Ollama
        let response = match client.generate_response(&user_input, &current_context, &conversation_history).await {
            Ok(response) => {
                // Stop the thinking animation
                stop_thinking_animation(thinking_handle);
                
                conversation_history.push(format!("Assistant: {}", response));
                response
            },
            Err(e) => {
                // Stop the thinking animation
                stop_thinking_animation(thinking_handle);
                
                println!("{}", format!("Error: {}", e).red());
                println!("{}", format!("API URL: {}/api/generate", client.get_api_url()).yellow());
                println!("{}", "Couldn't process API response. The model may have returned an unexpected format.".yellow());
                continue;
            }
        };
        
        // Check if response contains code suggestions
        println!("{}", "Analyzing response for code suggestions...".yellow());

        // Always display the response first so the user sees what the AI said
        println!("{}: {}", "Assistant".bright_blue(), response);
        
        // Then check for diffs separately
        if !response.contains("```") {
            // No code blocks found at all
            continue;
        }
        
        // Extract and print diff blocks (before parsing)
        let diff_blocks = diff_generator.extract_raw_diff_blocks(&response);
        if diff_blocks.is_empty() {
            // No diff suggestions found, just continue
            continue;
        }

        // Check if the code block was explicitly marked as a diff
        let has_explicit_diff = response.contains("```diff");
        
        if has_explicit_diff {
            println!("{}", format!("Found {} explicit diff suggestion(s):", diff_blocks.len()).green());
        } else {
            println!("{}", format!("Found {} code suggestion(s) that look like diffs:", diff_blocks.len()).green());
        }

        // Parse diffs from the extracted blocks
        let diffs = diff_generator.extract_diffs(&response);
        
        if !diffs.is_empty() {
            for (i, diff) in diffs.iter().enumerate() {
                println!("\n{} {}:", "Suggestion".bright_green(), i + 1);
                // Print directly without further formatting to preserve ANSI colors
                println!("{}", diff.display_diff());
                
                let options = vec!["Accept", "Reject"];
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Accept or reject this change?")
                    .default(0)
                    .items(&options)
                    .interact()?;
                
                match selection {
                    0 => {
                        // Accept the diff
                        println!("{}", "Applying changes...".green());
                        diff.apply()?;
                        println!("{}", format!("✅ Changes successfully applied to {}", diff.get_file_path().display()).green());
                    },
                    1 => {
                        // Reject the diff
                        println!("{}", "Changes rejected.".yellow());
                    },
                    _ => unreachable!(),
                }
            }
            
            // Update context after changes
            current_context = context_manager.get_context()?;
        } else {
            // No valid diffs could be parsed
            println!("{}", "Found code block(s) but couldn't parse valid diff(s).".yellow());
            println!("{}: {}", "Assistant".bright_blue(), response);
        }
    }
    
    println!("{}", "Thank you for using code-llm!".green());
    Ok(())
}

/// Get the path to the history file in the config directory
fn select_model_from_list(available_models: &[String]) -> Result<String> {
    // Create a list of available models for selection
    let model_choices: Vec<&str> = available_models.iter().map(AsRef::as_ref).collect();
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a model to use")
        .default(0)
        .items(&model_choices)
        .interact()?;
    
    // Get the selected model name
    let selected = available_models[selection].clone();
    println!("{}", format!("Selected model: {}", selected).green());
    
    Ok(selected)
}

fn get_history_file_path() -> Result<PathBuf> {
    let mut path = get_config_dir()?;
    
    // Add the history file name
    path.push("history");
    
    Ok(path)
}
