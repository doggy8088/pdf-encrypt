#!/usr/bin/env bash
# Builds the macOS installer package.
#
#   packaging/macos/build-pkg.sh <binary-path> <version> <output-pkg>
#
# Signing is enabled when these environment variables are set:
#   MACOS_APP_IDENTITY        e.g. "Developer ID Application: Duotify Inc. (XKF69LC6US)"
#                             signs the pdf-encrypt executable (hardened runtime, secure timestamp)
#   MACOS_INSTALLER_IDENTITY  e.g. "Developer ID Installer: Duotify Inc. (XKF69LC6US)"
#                             signs the .pkg
#   MACOS_KEYCHAIN            optional keychain that holds both identities
# Notarize the result with packaging/macos/notarize-pkg.sh.
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: $0 <binary-path> <version> <output-pkg>" >&2
  exit 2
fi

binary_path=$1
version=$2
output_pkg=$3
identifier="io.github.doggy8088.pdf-encrypt"

if [[ ! -x "$binary_path" ]]; then
  echo "binary not found or not executable: $binary_path" >&2
  exit 1
fi

keychain_args=()
if [[ -n "${MACOS_KEYCHAIN:-}" ]]; then
  keychain_args=(--keychain "$MACOS_KEYCHAIN")
fi

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

install_root="$work_dir/root"
mkdir -p "$install_root/usr/local/bin"
cp "$binary_path" "$install_root/usr/local/bin/pdf-encrypt"

if [[ -n "${MACOS_APP_IDENTITY:-}" ]]; then
  codesign \
    --force \
    --sign "$MACOS_APP_IDENTITY" \
    --identifier "$identifier" \
    --options runtime \
    --timestamp \
    ${keychain_args[@]+"${keychain_args[@]}"} \
    "$install_root/usr/local/bin/pdf-encrypt"
  codesign --verify --strict --verbose=2 "$install_root/usr/local/bin/pdf-encrypt"
fi

sign_args=()
if [[ -n "${MACOS_INSTALLER_IDENTITY:-}" ]]; then
  sign_args=(--sign "$MACOS_INSTALLER_IDENTITY" --timestamp ${keychain_args[@]+"${keychain_args[@]}"})
fi

pkgbuild \
  --root "$install_root" \
  --identifier "$identifier" \
  --version "$version" \
  --install-location "/" \
  ${sign_args[@]+"${sign_args[@]}"} \
  "$output_pkg"

if [[ -n "${MACOS_INSTALLER_IDENTITY:-}" ]]; then
  pkgutil --check-signature "$output_pkg"
fi
