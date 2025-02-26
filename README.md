# Code CLI

A CLI tool that works in an iterative chat-like style similar to "Claude Code", providing code suggestions based on your local directory context using Ollama models.

## Features

- Interactive chat interface for code assistance
- Integrates with locally running Ollama models
- Analyzes your code directory to provide context-aware suggestions
- Presents code changes as diffs for easy review
- Allows accepting, rejecting, or modifying suggested changes
- Respects .gitignore patterns for context building

## Prerequisites

- Rust (2021 edition)
- [Ollama](https://ollama.ai/) running locally with your preferred models

## Installation

Clone and build the project:

```bash
git clone https://github.com/yourusername/code-cli.git
cd code-cli
cargo build --release
```

The binary will be available at `target/release/code-cli`.

## Usage

Basic usage:

```bash
# Start interactive mode with default settings
code-cli

# Specify a different model
code-cli --model codellama

# Change the Ollama API endpoint
code-cli --api-url http://custom-ollama-host:11434
```

Commands:

```bash
# Initialize context (builds initial context for the current directory)
code-cli init
```

## How it Works

1. The CLI analyzes your current directory, respecting .gitignore patterns
2. It builds a context from your codebase that's sent to the Ollama model
3. You interact with the CLI by asking questions or requesting changes
4. The model responses are parsed for code suggestions in diff format
5. You can review, accept, or reject suggested changes
6. Accepted changes are applied to your codebase

## Configuration

The CLI uses sensible defaults but can be customized:

- Default model: llama3
- API endpoint: http://localhost:11434
- Max file size: 100KB per file
- Max context size: 8MB total

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.