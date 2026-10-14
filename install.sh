#!/bin/sh
# pdf-encrypt installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.sh | sh -s -- --version 0.3.0
#
# Options (environment variables work too):
#   --version <x.y.z>   PDF_ENCRYPT_VERSION      default: latest release
#   --to <dir>          PDF_ENCRYPT_INSTALL_DIR  default: $HOME/.local/bin
#
# The script downloads the raw executable from GitHub Releases, verifies its
# SHA-256 checksum, and installs it as <dir>/pdf-encrypt.

set -eu

REPO="doggy8088/pdf-encrypt"
BIN="pdf-encrypt"

say() { printf '%s\n' "pdf-encrypt-installer: $*"; }
err() { printf '%s\n' "pdf-encrypt-installer: error: $*" >&2; exit 1; }

usage() {
  cat <<EOF
Usage: install.sh [--version <x.y.z>] [--to <dir>]

  --version <x.y.z>  Install a specific release (default: latest)
  --to <dir>         Install directory (default: \$HOME/.local/bin)
  -h, --help         Show this help
EOF
}

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)

  case "$arch" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=aarch64 ;;
    *) err "unsupported CPU architecture: $arch" ;;
  esac

  case "$os" in
    Darwin)
      # A shell running under Rosetta reports x86_64 on Apple Silicon.
      if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
        arch=aarch64
      fi
      echo "$arch-apple-darwin"
      ;;
    Linux)
      # The musl builds are statically linked and run on any Linux distribution.
      echo "$arch-unknown-linux-musl"
      ;;
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
      err "on Windows, run: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
      ;;
    *) err "unsupported operating system: $os" ;;
  esac
}

download() {
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --https-only "$1" -O "$2"
  else
    err "curl or wget is required"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    err "sha256sum or shasum is required to verify the download"
  fi
}

main() {
  version=${PDF_ENCRYPT_VERSION:-}
  install_dir=${PDF_ENCRYPT_INSTALL_DIR:-$HOME/.local/bin}

  while [ $# -gt 0 ]; do
    case "$1" in
      --version) [ $# -ge 2 ] || err "--version needs a value"; version=$2; shift 2 ;;
      --version=*) version=${1#*=}; shift ;;
      --to) [ $# -ge 2 ] || err "--to needs a value"; install_dir=$2; shift 2 ;;
      --to=*) install_dir=${1#*=}; shift ;;
      -h | --help) usage; exit 0 ;;
      *) usage >&2; err "unknown option: $1" ;;
    esac
  done

  target=$(detect_target)
  asset="$BIN-$target"

  version=${version#v}
  if [ -z "$version" ] || [ "$version" = latest ]; then
    base_url="https://github.com/$REPO/releases/latest/download"
    label="latest"
  else
    base_url="https://github.com/$REPO/releases/download/v$version"
    label="v$version"
  fi

  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  say "downloading $asset ($label)"
  download "$base_url/$asset" "$tmp_dir/$asset" || err "download failed: $base_url/$asset"
  download "$base_url/$asset.sha256" "$tmp_dir/$asset.sha256" || err "download failed: $base_url/$asset.sha256"

  expected=$(awk '{print $1; exit}' "$tmp_dir/$asset.sha256")
  actual=$(sha256_of "$tmp_dir/$asset")
  [ "$expected" = "$actual" ] || err "checksum mismatch for $asset (expected $expected, got $actual)"
  say "checksum verified"

  mkdir -p "$install_dir"
  # Copy next to the destination first, then rename, so a running copy is replaced atomically.
  cp "$tmp_dir/$asset" "$install_dir/.$BIN.tmp.$$"
  chmod 755 "$install_dir/.$BIN.tmp.$$"
  mv -f "$install_dir/.$BIN.tmp.$$" "$install_dir/$BIN"

  say "installed $("$install_dir/$BIN" --version 2>/dev/null || echo "$BIN") to $install_dir/$BIN"

  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
      say "$install_dir is not on your PATH. Add this line to your shell profile:"
      # shellcheck disable=SC2016 # print $PATH literally for the user's profile
      printf '\n    export PATH="%s:$PATH"\n\n' "$install_dir"
      ;;
  esac
}

main "$@"
