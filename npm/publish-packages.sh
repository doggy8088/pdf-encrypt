#!/usr/bin/env bash
# Publishes the packages produced by npm/build-packages.mjs.
#
#   npm/publish-packages.sh <package-dir>...
#
# Pass the directories in the order build-packages.mjs prints them, so the
# platform packages exist before the main package that depends on them.
# Versions that are already on the registry are skipped, so reruns are safe.
# Prerelease versions are published under the "next" dist-tag. In GitHub
# Actions the packages are published with provenance.
set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <package-dir>..." >&2
  exit 2
fi

flags=(--access public)
if [[ "${GITHUB_ACTIONS:-}" == true ]]; then
  flags+=(--provenance)
fi

read_field() {
  node -p 'require(require("node:path").resolve(process.argv[1], "package.json"))[process.argv[2]]' "$1" "$2"
}

for dir in "$@"; do
  name=$(read_field "$dir" name)
  version=$(read_field "$dir" version)

  if npm view "$name@$version" version >/dev/null 2>&1; then
    echo "$name@$version is already published; skipping."
    continue
  fi

  tag=latest
  [[ "$version" == *-* ]] && tag=next

  echo "Publishing $name@$version (tag: $tag)"
  npm publish "$dir" "${flags[@]}" --tag "$tag"
done
