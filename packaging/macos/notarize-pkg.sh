#!/usr/bin/env bash
# Notarizes a signed .pkg with Apple, staples the ticket, and verifies it.
#
#   packaging/macos/notarize-pkg.sh <signed-pkg>
#
# Requires an App Store Connect API key:
#   APPLE_API_KEY_PATH   path to AuthKey_<KEY_ID>.p8
#   APPLE_API_KEY_ID     key ID
#   APPLE_API_ISSUER_ID  issuer ID
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <signed-pkg>" >&2
  exit 2
fi

pkg=$1
: "${APPLE_API_KEY_PATH:?APPLE_API_KEY_PATH is required}"
: "${APPLE_API_KEY_ID:?APPLE_API_KEY_ID is required}"
: "${APPLE_API_ISSUER_ID:?APPLE_API_ISSUER_ID is required}"

auth=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")

result=$(xcrun notarytool submit "$pkg" "${auth[@]}" --wait --timeout 1h --output-format json)
echo "$result"

submission_id=$(plutil -extract id raw -o - - <<<"$result")
status=$(plutil -extract status raw -o - - <<<"$result")

if [[ "$status" != "Accepted" ]]; then
  echo "Notarization finished with status: $status" >&2
  xcrun notarytool log "$submission_id" "${auth[@]}" >&2 || true
  exit 1
fi

xcrun stapler staple "$pkg"
xcrun stapler validate "$pkg"
spctl --assess --type install --verbose=4 "$pkg"
