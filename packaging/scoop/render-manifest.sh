#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 5 ]]; then
  echo "usage: $0 <version> <x64-sha256> <arm64-sha256> <template> <output>" >&2
  exit 2
fi

version=$1
x64_sha256=$2
arm64_sha256=$3
template=$4
output=$5

sed \
  -e "s/0\\.1\\.0/$version/g" \
  -e "s/REPLACE_WITH_X86_64_WINDOWS_MSVC_SHA256/$x64_sha256/g" \
  -e "s/REPLACE_WITH_AARCH64_WINDOWS_MSVC_SHA256/$arm64_sha256/g" \
  "$template" > "$output"
