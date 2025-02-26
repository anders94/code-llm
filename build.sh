#!/bin/bash
set -e

# Build and install code-cli

echo "Building code-cli..."
cargo build --release

echo "Installing code-cli to /usr/local/bin (might require sudo)..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sudo cp target/release/code-cli /usr/local/bin/
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    sudo cp target/release/code-cli /usr/local/bin/
else
    echo "Unsupported OS. Please manually copy the binary from target/release/code-cli to your PATH."
    exit 1
fi

echo "Installation complete! Run 'code-cli' to start using the tool."
echo "Make sure you have Ollama installed and running: https://ollama.ai/"