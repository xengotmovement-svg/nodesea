#!/bin/sh
# Install nodesea — download the latest release binary.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Dunqing/nodesea/main/install.sh | sh
#
# Options:
#   NODESEA_VERSION  — specific version (default: latest)
#   INSTALL_DIR      — install directory (default: /usr/local/bin)

set -e

REPO="Dunqing/nodesea"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect platform.
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) PLATFORM="darwin" ;;
  linux)  PLATFORM="linux" ;;
  *)      echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH_NAME="x64" ;;
  aarch64|arm64) ARCH_NAME="arm64" ;;
  *)             echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

# Determine version.
if [ -z "$NODESEA_VERSION" ]; then
  NODESEA_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)
  if [ -z "$NODESEA_VERSION" ]; then
    echo "Failed to fetch latest version" >&2
    exit 1
  fi
fi

NAME="nodesea-${PLATFORM}-${ARCH_NAME}"
URL="https://github.com/${REPO}/releases/download/${NODESEA_VERSION}/${NAME}.tar.gz"

echo "Installing nodesea ${NODESEA_VERSION} (${PLATFORM}-${ARCH_NAME})..."
echo "  ${URL}"

# Download and extract.
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" | tar xz -C "$TMPDIR"

# Install.
mkdir -p "$INSTALL_DIR"
mv "$TMPDIR/nodesea" "$INSTALL_DIR/nodesea"
chmod +x "$INSTALL_DIR/nodesea"

echo "Installed to ${INSTALL_DIR}/nodesea"
echo ""
nodesea --version 2>/dev/null || echo "Add ${INSTALL_DIR} to your PATH if needed."
