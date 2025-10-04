#!/bin/bash
# Build julie-codesearch and julie-semantic for current platform
# Run this on macOS, Linux, and Windows (via Git Bash or WSL)

set -e

echo "🔨 Building julie binaries for current platform..."

# Detect platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    PLATFORM="macos-$(uname -m)"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    PLATFORM="linux-$(uname -m)"
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    PLATFORM="windows-x64"
else
    PLATFORM="unknown"
fi

echo "📦 Platform detected: $PLATFORM"

# Build binaries
echo "🔨 Building julie-codesearch..."
cargo build --release --bin julie-codesearch

echo "🔨 Building julie-semantic..."
cargo build --release --bin julie-semantic

# Show binary info
echo ""
echo "✅ Build complete!"
echo ""
echo "📍 Binary locations:"
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    ls -lh target/release/julie-codesearch.exe target/release/julie-semantic.exe
    echo ""
    echo "📋 Next steps:"
    echo "  Copy these files to CodeSearch:"
    echo "    target/release/julie-codesearch.exe → bin/julie-binaries/julie-codesearch-windows-x64.exe"
    echo "    target/release/julie-semantic.exe → bin/julie-binaries/julie-semantic-windows-x64.exe"
else
    ls -lh target/release/julie-codesearch target/release/julie-semantic
    echo ""
    echo "📋 Next steps:"
    echo "  Copy these files to CodeSearch:"
    echo "    target/release/julie-codesearch → bin/julie-binaries/julie-codesearch-$PLATFORM"
    echo "    target/release/julie-semantic → bin/julie-binaries/julie-semantic-$PLATFORM"
fi
