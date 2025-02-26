use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Input, Select};

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
    
    println!("{}", format!("Welcome to code-cli! Using model: {}", model).green());
    println!("{}", "Type your questions/requests or 'exit' to quit.".blue());
    
    let mut conversation_history = Vec::new();
    let mut current_context = context_manager.get_context()?;
    
    loop {
        // Get user input, handling Ctrl+D as an exit signal
        let user_input: String = match Input::with_theme(&ColorfulTheme::default())
            .with_prompt("You")
            .allow_empty(true)
            .interact_text() {
                Ok(input) => input,
                Err(e) => {
                    // Check if this is EOF (Ctrl+D)
                    if e.to_string().contains("EOF") || e.to_string().contains("end of file") {
                        println!("\n{}", "Exiting due to Ctrl+D".blue());
                        break;
                    }
                    // For other errors, re-raise them
                    return Err(e.into());
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
    
    println!("{}", "Thank you for using code-cli!".green());
    Ok(())
}
