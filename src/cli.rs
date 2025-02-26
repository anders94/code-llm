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
        let user_input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("You")
            .interact_text()?;
            
        if user_input.trim().to_lowercase() == "exit" {
            break;
        }
        
        conversation_history.push(format!("User: {}", user_input));
        
        println!("{}", "Thinking...".yellow());
        
        // Get response from Ollama
        let response = client.generate_response(&user_input, &current_context, &conversation_history).await?;
        conversation_history.push(format!("Assistant: {}", response));
        
        // Check if response contains code suggestions
        let diffs = diff_generator.extract_diffs(&response);
        
        if !diffs.is_empty() {
            for (i, diff) in diffs.iter().enumerate() {
                println!("\n{} {}:", "Suggestion".bright_green(), i + 1);
                println!("{}", diff.display_diff());
                
                let options = vec!["Accept", "Reject", "Modify"];
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("What would you like to do with this suggestion?")
                    .default(0)
                    .items(&options)
                    .interact()?;
                
                match selection {
                    0 => {
                        // Accept the diff
                        println!("{}", "Applying changes...".green());
                        diff.apply()?;
                    },
                    1 => {
                        // Reject the diff
                        println!("{}", "Changes rejected.".yellow());
                    },
                    2 => {
                        // Modify the diff
                        println!("{}", "TODO: Implement modification of diffs".red());
                        // This would involve opening the diff in an editor or providing a way to edit it
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