use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use dirs::home_dir;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::fs;
use std::path::PathBuf;

use crate::api::OllamaClient;
use crate::context::ContextManager;
use crate::diff::{DiffGenerator, DiffAction};

#[derive(Parser)]
#[clap(author, version, about)]
pub struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,

    /// The model to use for code suggestions
    #[clap(short, long, default_value = "llama3")]
    model: String,

    /// Ollama API endpoint URL
    #[clap(long, default_value = "http://localhost:11434")]
    api_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new context
    Init,
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let model = cli.model;
    let api_url = cli.api_url;

    match &cli.command {
        Some(Commands::Init) => {
            println!("{}", "Initializing new context...".green());
            // Initialize code context logic would go here
            return Ok(());
        }
        None => {
            // Interactive mode
            run_interactive_mode(&model, &api_url).await?;
        }
    }

    Ok(())
}

async fn run_interactive_mode(model: &str, api_url: &str) -> Result<()> {
    let client = OllamaClient::new(api_url, model);
    let context_manager = ContextManager::new(".")?;
    let diff_generator = DiffGenerator::new();
    
    println!("{}", format!("Welcome to code-llm! Using model: {}", model).green());
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
        
        if user_input.trim().to_lowercase() == "exit" {
            break;
        }
        
        conversation_history.push(format!("User: {}", user_input));
        
        println!("{}", "Thinking...".yellow());
        
        // Get response from Ollama
        let response = match client.generate_response(&user_input, &current_context, &conversation_history).await {
            Ok(response) => {
                conversation_history.push(format!("Assistant: {}", response));
                response
            },
            Err(e) => {
                println!("{}", format!("Error: {}", e).red());
                println!("{}", format!("API URL: {}/api/generate", client.get_api_url()).yellow());
                println!("{}", "Couldn't process API response. The model may have returned an unexpected format.".yellow());
                continue;
            }
        };
        
        // Check if response contains code suggestions
        println!("{}", "Analyzing response for code suggestions...".yellow());

        // Extract and print diff blocks (before parsing)
        let diff_blocks = diff_generator.extract_raw_diff_blocks(&response);
        if !diff_blocks.is_empty() {
            println!("{}", format!("Found {} code suggestion(s):", diff_blocks.len()).green());
        } else {
            println!("{}", "No code suggestions found in response. The model might not be using the diff format.".yellow());
            println!("{}: {}", "Assistant".bright_blue(), response);
            continue;
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
            // Just display the response if no code suggestions
            println!("{}: {}", "Assistant".bright_blue(), response);
        }
    }
    
    println!("{}", "Thank you for using code-llm!".green());
    Ok(())
}

/// Get the path to the history file in the user's home directory
fn get_history_file_path() -> Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
    path.push(".code-llm_history");
    
    // Make sure the parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    
    Ok(path)
}
