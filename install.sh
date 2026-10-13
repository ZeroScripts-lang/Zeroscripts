#!/bin/sh
# ZeroScript installer — POSIX sh, auto-detect OS/arch.
# Usage: curl -fsSL <raw-url>/install.sh | sh
# Env:   ZSC_INSTALL_DIR (default: $HOME/.zeroscript)
set -eu

REPO="ZeroScripts-lang/Zeroscripts"
PREFIX="${ZSC_INSTALL_DIR:-$HOME/.zeroscript}"
BIN="$PREFIX/bin"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux|Android) os_key="unknown-linux-musl" ;;
    Darwin) os_key="apple-darwin" ;;
    *)
        echo "error: unsupported OS '$os' — download the Windows zip from the GitHub release" >&2
        exit 1
        ;;
esac

case "$arch" in
    x86_64|amd64)  arch_key="x86_64" ;;
    aarch64|arm64) arch_key="aarch64" ;;
    *)
        echo "error: unsupported architecture '$arch'" >&2
        exit 1
        ;;
esac

target="${arch_key}-${os_key}"

# Latest release tag (no jq dependency — grep/sed only).
if [ -n "${ZSC_TAG:-}" ]; then
    tag="$ZSC_TAG"
else
    tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -m1 '"tag_name":' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
    [ -n "$tag" ] || { echo "error: could not resolve latest release tag" >&2; exit 1; }
fi

asset="zeroscript-${tag}-${target}.tar.gz"
url="${ZSC_DOWNLOAD_BASE:-https://github.com/$REPO/releases/download}/${tag}/${asset}"

echo "installing ZeroScript $tag ($target)"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fSL -o "$tmp/$asset" "$url"
tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$BIN"
# tarball layout: zeroscript-<tag>-<target>/{zsc,zpm}
cp "$tmp"/*/zsc "$tmp"/*/zpm "$BIN/"
chmod +x "$BIN/zsc" "$BIN/zpm"

echo "installed to $BIN"
case ":$PATH:" in
    *":$BIN:"*) ;;
    *)
        echo "add to PATH:"
        echo "  export PATH=\"$BIN:\$PATH\""
        ;;
esac
"$BIN/zsc" --version
"$BIN/zpm" --version
