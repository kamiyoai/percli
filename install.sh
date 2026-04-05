#!/usr/bin/env bash
# percli installer — downloads the latest release binary for your platform.
# Usage: curl -fsSL https://raw.githubusercontent.com/kamiyoai/percli/master/install.sh | bash

set -euo pipefail

REPO="kamiyoai/percli"
INSTALL_DIR="${PERCLI_INSTALL_DIR:-$HOME/.percli/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *)      echo "error: unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) arch="x86_64" ;;
  aarch64|arm64) arch="aarch64" ;;
  *)             echo "error: unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${arch}-${os}"

# Get latest release tag
echo "Fetching latest release..."
TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"

if [ -z "$TAG" ]; then
  echo "error: could not determine latest release" >&2
  exit 1
fi

ARCHIVE="percli-${TAG}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"

# Download and extract
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading percli ${TAG} for ${TARGET}..."
curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"

mkdir -p "$INSTALL_DIR"
tar xzf "${TMPDIR}/${ARCHIVE}" -C "$INSTALL_DIR"
chmod +x "${INSTALL_DIR}/percli"

echo ""
echo "Installed percli ${TAG} to ${INSTALL_DIR}/percli"

# Check if in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
  echo ""
  echo "Add to your PATH:"
  echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  echo ""
  echo "Or add to your shell profile:"
  SHELL_NAME="$(basename "$SHELL")"
  case "$SHELL_NAME" in
    zsh)  echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.zshrc" ;;
    bash) echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc" ;;
    fish) echo "  set -Ux fish_user_paths ${INSTALL_DIR} \$fish_user_paths" ;;
    *)    echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.profile" ;;
  esac
fi

echo ""
echo "Run 'percli --help' to get started."
