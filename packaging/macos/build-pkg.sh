#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: $0 <binary-path> <version> <output-pkg>" >&2
  exit 2
fi

binary_path=$1
version=$2
output_pkg=$3

if [[ ! -x "$binary_path" ]]; then
  echo "binary not found or not executable: $binary_path" >&2
  exit 1
fi

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

install_root="$work_dir/root"
mkdir -p "$install_root/usr/local/bin"
cp "$binary_path" "$install_root/usr/local/bin/pdf-encrypt"

pkgbuild \
  --root "$install_root" \
  --identifier "io.github.doggy8088.pdf-encrypt" \
  --version "$version" \
  --install-location "/" \
  "$output_pkg"
