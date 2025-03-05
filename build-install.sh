#!/bin/bash
set -e

# Build and install code-llm

echo "Building code-llm..."
cargo build --release

echo "Installing code-llm to /usr/local/bin (might require sudo)..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sudo cp target/release/code-llm /usr/local/bin/
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    sudo cp target/release/code-llm /usr/local/bin/
else
    echo "Unsupported OS. Please manually copy the binary from target/release/code-llm to your PATH."
    exit 1
fi

echo "Installation complete! Run 'code-llm' to start using the tool."
echo "Make sure you have Ollama installed and running: https://ollama.ai/"
